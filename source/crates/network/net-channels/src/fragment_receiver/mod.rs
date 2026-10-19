//! Fragment validation, reassembly, and decompression for channel receives.

use bevy_platform::collections::HashMap;
use campfire_net_core::Instant;

use crate::channel_receive_error::ChannelReceiveError;

type Result<T> = core::result::Result<T, ChannelReceiveError>;
use crate::compression::decompress_payload;
#[cfg(feature = "compression_lz4")]
use crate::compression_algorithm::CompressionAlgorithm;
use crate::compression_config::CompressionConfig;
use crate::fragment_compression::FragmentCompression;
use crate::fragment_data::FragmentData;
use crate::message_id::MessageId;
use crate::packet::MAX_FRAGMENTS;
use crate::packet_error::PacketError;
use crate::receive_budget::ReceiveBudget;
use bytes::Bytes;
use campfire_net_bytes::Writer;
use campfire_net_core::NetTick;
use tracing::trace;

/// `FragmentReceiver` is used to reconstruct fragmented messages
///
/// Each sender derives its fragment size from its own link, so the receiver takes the size of a
/// message's fragments from those fragments instead of from its own link.
#[derive(Debug)]
pub(crate) struct FragmentReceiver {
    fragment_messages: HashMap<MessageId, FragmentConstructor>,
    /// The bytes the partial messages hold, as charged to the receive budget.
    held: usize,
}

/// What a received fragment is checked and stored against.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FragmentContext {
    pub(crate) expiry: PartialExpiry,
    pub(crate) compression: CompressionConfig,
    /// The channel's largest message, uncompressed.
    pub(crate) max_message_size: usize,
    pub(crate) reservation: Reservation,
}

/// When a partial message expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartialExpiry {
    /// The partial waits for its fragments forever: its channel resends or supersedes them.
    Never,
    /// The partial ages from this arrival of one of its fragments.
    Since(Instant),
}

/// How a new partial message's buffer is charged to the receive budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reservation {
    /// Refused when the budget has no room for it.
    Limited,
    /// Charged even past the limit: the message an ordered channel delivers next, without which
    /// nothing waiting behind it can be delivered to free room.
    Head,
}

/// What became of a received fragment.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FragmentOutcome {
    /// The fragment is stored, or was received before; the message is not complete yet.
    Pending,
    /// The fragment completed the message, which carries the tick of its first fragment.
    Complete { tick: NetTick, bytes: Bytes },
    /// The fragment would have opened a partial message the receive budget has no room for.
    OverBudget,
}

impl FragmentReceiver {
    pub(crate) fn new() -> Self {
        Self {
            fragment_messages: HashMap::default(),
            held: 0,
        }
    }

    pub(crate) const fn held(&self) -> usize {
        self.held
    }

    /// Discards each expiring partial message whose latest fragment arrived at or before
    /// `cleanup_time`: its other fragments were most likely lost.
    pub(crate) fn cleanup(&mut self, cleanup_time: Instant) {
        let held = &mut self.held;
        self.fragment_messages.retain(|_, constructor| {
            let keep = match constructor.expiry {
                PartialExpiry::Never => true,
                PartialExpiry::Since(received) => received > cleanup_time,
            };
            if !keep {
                *held -= constructor.charged;
            }
            keep
        });
    }

    /// Drops the partial messages older than `newest`, which a sequenced channel never delivers.
    pub(crate) fn discard_older_than(&mut self, newest: MessageId) {
        let held = &mut self.held;
        self.fragment_messages.retain(|&id, constructor| {
            let keep = id - newest >= 0;
            if !keep {
                *held -= constructor.charged;
            }
            keep
        });
    }

    fn remove(&mut self, message_id: MessageId) {
        if let Some(constructor) = self.fragment_messages.remove(&message_id) {
            self.held -= constructor.charged;
        }
    }

    /// Receive a fragment of a FragmentData message.
    ///
    /// When we complete the final message by aggregating all fragments, we will return the
    /// `remote_sent_tick` associated with the first fragment received.
    ///
    /// A fragment whose message could not fit `max_message_size` is refused before anything is
    /// allocated for it: its count alone when it comes first, and `(count - 1) * fragment_size + 1`
    /// once a non-final fragment gives the size.
    pub(crate) fn receive_fragment(
        &mut self,
        fragment: FragmentData,
        remote_sent_tick: NetTick,
        context: &FragmentContext,
        budget: &mut ReceiveBudget,
    ) -> Result<FragmentOutcome> {
        let num_fragments = fragment.num_fragments.0;
        let fragment_index = fragment.fragment_index.0;
        if num_fragments == 0 {
            return Err(ChannelReceiveError::InvalidFragmentCount { num_fragments });
        }
        if fragment_index >= num_fragments {
            return Err(ChannelReceiveError::InvalidFragmentIndex {
                fragment_index,
                num_fragments,
            });
        }
        // Every fragment holds at least one byte, so a message of more fragments than bytes
        // allowed can not fit; no sender splits a message into more than `MAX_FRAGMENTS`.
        let num_fragments = usize::try_from(num_fragments)
            .ok()
            .filter(|&count| count <= context.max_message_size.min(MAX_FRAGMENTS))
            .ok_or(ChannelReceiveError::FragmentCountTooLarge {
                num_fragments,
                max_message_size: context.max_message_size,
            })?;
        let fragment_index = usize::try_from(fragment_index).unwrap();

        let message_id = fragment.message_id;
        let fragment_compression = fragment.compression;
        let fragment_message = self
            .fragment_messages
            .entry(message_id)
            .or_insert_with(|| FragmentConstructor::new(remote_sent_tick, num_fragments));
        if fragment_message.num_fragments != num_fragments {
            return Err(ChannelReceiveError::FragmentCountMismatch {
                expected: fragment_message.num_fragments,
                actual: num_fragments,
            });
        }
        if let Some(fragment_compression) = fragment_compression {
            fragment_message.set_compression(fragment_compression)?;
        } else if fragment_index == 0 {
            return Err(ChannelReceiveError::MissingFragmentCompression);
        }

        let charged_before = fragment_message.charged;
        let stored =
            fragment_message.receive_fragment(fragment_index, fragment.bytes, context, budget);
        self.held += fragment_message.charged - charged_before;
        let empty = fragment_message.num_received_fragments == 0;
        let fragment_compression = fragment_message.compression;
        let (tick, payload) = match stored {
            Ok(Stored::Pending) => return Ok(FragmentOutcome::Pending),
            Ok(Stored::Complete { tick, bytes }) => (tick, bytes),
            Ok(Stored::OverBudget) => {
                if empty {
                    self.remove(message_id);
                }
                return Ok(FragmentOutcome::OverBudget);
            }
            Err(error) => {
                self.remove(message_id);
                return Err(error);
            }
        };
        self.remove(message_id);
        {
            let fragment_compression =
                fragment_compression.ok_or(ChannelReceiveError::MissingFragmentCompression)?;
            let compression = CompressionConfig {
                max_decompressed_payload_size: context
                    .compression
                    .max_decompressed_payload_size
                    .min(context.max_message_size),
                ..context.compression
            };
            decompress_fragment_payload(fragment_compression, payload, compression)
                .map(|bytes| FragmentOutcome::Complete { tick, bytes })
        }
    }
}

/// What a constructor did with one fragment.
#[derive(Debug)]
enum Stored {
    Pending,
    Complete { tick: NetTick, bytes: Bytes },
    OverBudget,
}

#[derive(Debug, Clone)]
/// Data structure to reconstruct a single fragmented message from individual fragments
pub(super) struct FragmentConstructor {
    num_fragments: usize,
    num_received_fragments: usize,
    layout: FragmentLayout,

    tick: NetTick,
    compression: Option<FragmentCompression>,
    expiry: PartialExpiry,
    /// The bytes charged to the receive budget for this partial.
    charged: usize,
}

/// Where each fragment of a message goes in the reassembled message.
#[derive(Debug, Clone)]
enum FragmentLayout {
    /// No non-final fragment was received, so the sender's fragment size is not known yet. The
    /// final fragment waits here when it arrives first, because its offset depends on that size;
    /// it is the only fragment that can have arrived, so nothing else records receipts.
    Unknown {
        final_fragment: Option<Bytes>,
    },
    Known(ReassemblyBuffer),
}

/// The message bytes, with the fragment size of the first non-final fragment received. The sender
/// splits a message into fragments of one size, so every other non-final fragment must have it too.
#[derive(Debug, Clone)]
struct ReassemblyBuffer {
    fragment_size: usize,
    bytes: Vec<u8>,
    /// Bit `i % 64` of word `i / 64` is set once fragment `i` is written.
    received: Vec<u64>,
}

impl FragmentConstructor {
    pub(super) const fn new(tick: NetTick, num_fragments: usize) -> Self {
        Self {
            num_fragments,
            num_received_fragments: 0,
            layout: FragmentLayout::Unknown {
                final_fragment: None,
            },
            tick,
            compression: None,
            expiry: PartialExpiry::Never,
            charged: 0,
        }
    }

    pub(super) fn set_compression(&mut self, compression: FragmentCompression) -> Result<()> {
        if let Some(expected) = self.compression {
            if expected != compression {
                return Err(ChannelReceiveError::FragmentCompressionMismatch {
                    expected: expected.as_str(),
                    actual: compression.as_str(),
                });
            }
        } else {
            self.compression = Some(compression);
        }
        Ok(())
    }

    fn receive_fragment(
        &mut self,
        fragment_index: usize,
        bytes: Bytes,
        context: &FragmentContext,
        budget: &mut ReceiveBudget,
    ) -> Result<Stored> {
        self.expiry = context.expiry;
        let max_message_size = context.max_message_size;
        let is_last_fragment = fragment_index == self.num_fragments - 1;
        if !is_last_fragment {
            if !self.learn_fragment_size(bytes.len(), context, budget)? {
                return Ok(Stored::OverBudget);
            }
        } else if let FragmentLayout::Known(buffer) = &self.layout
            && bytes.len() > buffer.fragment_size
        {
            return Err(ChannelReceiveError::InvalidFinalFragmentSize {
                actual: bytes.len(),
                max: buffer.fragment_size,
            });
        }

        match &mut self.layout {
            FragmentLayout::Known(buffer) => {
                if !buffer.mark_received(fragment_index) {
                    return Ok(Stored::Pending);
                }
                buffer.write(fragment_index, is_last_fragment, &bytes);
            }
            FragmentLayout::Unknown { final_fragment } => {
                if final_fragment.is_some() {
                    return Ok(Stored::Pending);
                }
                // The other fragments hold at least a byte each before it.
                let min_size = self.num_fragments - 1 + bytes.len();
                if min_size > max_message_size {
                    return Err(ChannelReceiveError::MessageTooLarge {
                        min_size,
                        max_message_size,
                    });
                }
                // The only fragment is also the whole message.
                if self.num_fragments == 1 {
                    return Ok(Stored::Complete {
                        tick: self.tick,
                        bytes,
                    });
                }
                if !reserve(budget, context.reservation, bytes.len()) {
                    return Ok(Stored::OverBudget);
                }
                self.charged = bytes.len();
                *final_fragment = Some(bytes);
            }
        }
        self.num_received_fragments += 1;

        if self.num_received_fragments == self.num_fragments {
            trace!("Received all fragments!");
            let FragmentLayout::Known(buffer) = &mut self.layout else {
                unreachable!("a message of several fragments has a non-final fragment");
            };
            let payload = core::mem::take(&mut buffer.bytes);
            return Ok(Stored::Complete {
                tick: self.tick,
                bytes: payload.into(),
            });
        }

        Ok(Stored::Pending)
    }

    /// Checks a non-final fragment's size against the message's fragment size, or makes it the
    /// message's fragment size if it is the first non-final fragment, and returns false when the
    /// budget has no room for the buffer that size needs.
    fn learn_fragment_size(
        &mut self,
        fragment_size: usize,
        context: &FragmentContext,
        budget: &mut ReceiveBudget,
    ) -> Result<bool> {
        let max_message_size = context.max_message_size;
        let final_fragment = match &mut self.layout {
            FragmentLayout::Known(buffer) => {
                return if fragment_size == buffer.fragment_size {
                    Ok(true)
                } else {
                    Err(ChannelReceiveError::InvalidNonFinalFragmentSize {
                        actual: fragment_size,
                        expected: buffer.fragment_size,
                    })
                };
            }
            FragmentLayout::Unknown { final_fragment } => final_fragment,
        };
        if fragment_size == 0 {
            return Err(ChannelReceiveError::EmptyNonFinalFragment);
        }
        if let Some(final_fragment) = final_fragment
            && final_fragment.len() > fragment_size
        {
            return Err(ChannelReceiveError::InvalidFinalFragmentSize {
                actual: final_fragment.len(),
                max: fragment_size,
            });
        }
        // The smallest message these fragments describe: every non-final fragment full, and a
        // final fragment of one byte, or of its own length once it arrived.
        let final_len = final_fragment.as_ref().map_or(1, Bytes::len);
        let min_size = (self.num_fragments - 1)
            .checked_mul(fragment_size)
            .and_then(|size| size.checked_add(final_len))
            .filter(|&size| size <= max_message_size)
            .ok_or_else(|| ChannelReceiveError::MessageTooLarge {
                min_size: (self.num_fragments - 1)
                    .saturating_mul(fragment_size)
                    .saturating_add(final_len),
                max_message_size,
            })?;
        // At most `max_message_size + fragment_size - 1` bytes, by the check above.
        let capacity = min_size - final_len + fragment_size;
        // A final fragment already charged its own length, which the buffer takes over.
        if !reserve(
            budget,
            context.reservation,
            capacity.saturating_sub(self.charged),
        ) {
            return Ok(false);
        }
        self.charged = self.charged.max(capacity);

        let mut buffer = ReassemblyBuffer {
            fragment_size,
            bytes: vec![0; capacity],
            received: vec![0; self.num_fragments.div_ceil(64)],
        };
        if let Some(final_fragment) = final_fragment.take() {
            buffer.mark_received(self.num_fragments - 1);
            buffer.write(self.num_fragments - 1, true, &final_fragment);
        }
        self.layout = FragmentLayout::Known(buffer);
        Ok(true)
    }
}

const fn reserve(budget: &mut ReceiveBudget, reservation: Reservation, bytes: usize) -> bool {
    match reservation {
        Reservation::Limited => budget.try_charge(bytes),
        Reservation::Head => {
            budget.charge(bytes);
            true
        }
    }
}

impl ReassemblyBuffer {
    /// Records fragment `index` as received, and returns false when it was received before.
    fn mark_received(&mut self, index: usize) -> bool {
        let (word, bit) = (index / 64, 1 << (index % 64));
        let new = self.received[word] & bit == 0;
        self.received[word] |= bit;
        new
    }

    /// `fragment` must hold `fragment_size` bytes, or at most that many if it is the final one.
    fn write(&mut self, fragment_index: usize, is_last_fragment: bool, fragment: &[u8]) {
        // Cannot overflow: the buffer holds `num_fragments * fragment_size` bytes until the final
        // fragment shortens it, and every non-final fragment lies before the final one.
        let start = fragment_index * self.fragment_size;
        let end = start + fragment.len();
        if is_last_fragment {
            self.bytes.truncate(end);
        }
        self.bytes[start..end].copy_from_slice(fragment);
    }
}

fn decompress_fragment_payload(
    compression: FragmentCompression,
    payload: Bytes,
    config: CompressionConfig,
) -> Result<Bytes> {
    match compression {
        FragmentCompression::None => Ok(payload),
        FragmentCompression::Lz4 => {
            let config = config_for_fragment_compression(compression, config)?;
            let mut out = Writer::with_capacity(0);
            decompress_payload(payload.as_ref(), config, &mut out)
                .map_err(|error| map_fragment_decompression_error(&error))?;
            Ok(out.into_bytes())
        }
    }
}

#[cfg_attr(
    not(feature = "compression_lz4"),
    expect(
        clippy::missing_const_for_fn,
        reason = "the lz4 arm calls a non-const fn"
    )
)]
fn config_for_fragment_compression(
    compression: FragmentCompression,
    config: CompressionConfig,
) -> Result<CompressionConfig> {
    match compression {
        FragmentCompression::None => Ok(config),
        FragmentCompression::Lz4 => {
            #[cfg(feature = "compression_lz4")]
            {
                if config.algorithm == Some(CompressionAlgorithm::Lz4) {
                    Ok(config)
                } else {
                    Err(ChannelReceiveError::UnsupportedFragmentCompression {
                        compression: compression.as_str(),
                    })
                }
            }
            #[cfg(not(feature = "compression_lz4"))]
            {
                Err(ChannelReceiveError::UnsupportedFragmentCompression {
                    compression: compression.as_str(),
                })
            }
        }
    }
}

const fn map_fragment_decompression_error(error: &PacketError) -> ChannelReceiveError {
    match *error {
        PacketError::UnsupportedCompression => {
            ChannelReceiveError::UnsupportedFragmentCompression {
                compression: FragmentCompression::Lz4.as_str(),
            }
        }
        PacketError::DecompressionFailed => ChannelReceiveError::FragmentDecompressionFailed,
        PacketError::DecompressedPayloadTooLarge { actual, limit } => {
            ChannelReceiveError::FragmentDecompressedPayloadTooLarge { actual, limit }
        }
        _ => ChannelReceiveError::FragmentDecompressionFailed,
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::fragment_receiver::FragmentReceiver;

    impl FragmentReceiver {
        pub(crate) fn partial_len(&self) -> usize {
            self.fragment_messages.len()
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod bench;
#[cfg(test)]
mod tests;
