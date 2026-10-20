//! Fragment validation, reassembly, and decompression for channel receives.

use bevy_platform::collections::HashMap;
use campfire_net_core::Instant;

use crate::channel_receive_error::ChannelReceiveError;

type Result<T> = core::result::Result<T, ChannelReceiveError>;
use crate::compression::decompress_payload;
#[cfg(feature = "compression_lz4")]
use crate::compression_algorithm::CompressionAlgorithm;
use crate::compression_setup::CompressionSetup;
use crate::fragment_body::FragmentBody;
use crate::fragment_compression::FragmentCompression;
use crate::message_id::MessageId;
use crate::packet::MAX_FRAGMENTS;
use crate::packet_error::PacketError;
use crate::receive_budget::ReceiveBudget;
use bytes::Bytes;
use campfire_net_bytes::Writer;
use campfire_net_core::NetTick;
use tracing::trace;

/// The fragmented messages of a channel being built again, each taking its fragments' length from
/// its fragments, as its sender derives it from its own link (11-net-stack/transport.md, A
/// receiver learns each message's fragment size from its fragments).
#[derive(Debug)]
pub(crate) struct FragmentReceiver {
    fragment_messages: HashMap<MessageId, FragmentConstructor>,
    /// The bytes the partial messages hold, as charged to the receive budget.
    held: usize,
}

/// What a received fragment is checked and stored against.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FragmentCx {
    pub(crate) expiry: PartialExpiry,
    pub(crate) compression: CompressionSetup,
    /// The channel's largest message, uncompressed.
    pub(crate) max_message_len: usize,
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
    /// A fragment whose message could not fit `max_message_len` is refused before anything is
    /// allocated for it: its count alone when it comes first, and `(count - 1) * fragment_len + 1`
    /// once a non-final fragment gives the size.
    pub(crate) fn receive_fragment(
        &mut self,
        fragment: FragmentBody,
        remote_sent_tick: NetTick,
        cx: &FragmentCx,
        budget: &mut ReceiveBudget,
    ) -> Result<FragmentOutcome> {
        let fragments = fragment.fragments.0;
        let fragment_index = fragment.fragment_index.0;
        if fragments == 0 {
            return Err(ChannelReceiveError::InvalidFragmentCount { fragments });
        }
        if fragment_index >= fragments {
            return Err(ChannelReceiveError::InvalidFragmentIndex {
                fragment_index,
                fragments,
            });
        }
        // Every fragment holds at least one byte, so a message of more fragments than bytes
        // allowed can not fit; no sender splits a message into more than `MAX_FRAGMENTS`.
        let fragments = usize::try_from(fragments)
            .ok()
            .filter(|&count| count <= cx.max_message_len.min(MAX_FRAGMENTS))
            .ok_or(ChannelReceiveError::FragmentCountTooLarge {
                fragments,
                max_message_len: cx.max_message_len,
            })?;
        let fragment_index = usize::try_from(fragment_index).unwrap();

        let message_id = fragment.message_id;
        let fragment_compression = fragment.compression;
        let fragment_message = self
            .fragment_messages
            .entry(message_id)
            .or_insert_with(|| FragmentConstructor::new(remote_sent_tick, fragments));
        if fragment_message.fragments != fragments {
            return Err(ChannelReceiveError::FragmentCountMismatch {
                expected: fragment_message.fragments,
                actual: fragments,
            });
        }
        if let Some(fragment_compression) = fragment_compression {
            fragment_message.set_compression(fragment_compression)?;
        } else if fragment_index == 0 {
            return Err(ChannelReceiveError::MissingFragmentCompression);
        }

        let charged_before = fragment_message.charged;
        let stored = fragment_message.receive_fragment(fragment_index, fragment.bytes, cx, budget);
        self.held += fragment_message.charged - charged_before;
        let empty = fragment_message.received_fragments == 0;
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
            let compression = CompressionSetup {
                max_decompressed_payload_len: cx
                    .compression
                    .max_decompressed_payload_len
                    .min(cx.max_message_len),
                ..cx.compression
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
    fragments: usize,
    received_fragments: usize,
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
    fragment_len: usize,
    bytes: Vec<u8>,
    /// Bit `i % 64` of word `i / 64` is set once fragment `i` is written.
    received: Vec<u64>,
}

impl FragmentConstructor {
    pub(super) const fn new(tick: NetTick, fragments: usize) -> Self {
        Self {
            fragments,
            received_fragments: 0,
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
        cx: &FragmentCx,
        budget: &mut ReceiveBudget,
    ) -> Result<Stored> {
        self.expiry = cx.expiry;
        let max_message_len = cx.max_message_len;
        let is_last_fragment = fragment_index == self.fragments - 1;
        if !is_last_fragment {
            if !self.learn_fragment_len(bytes.len(), cx, budget)? {
                return Ok(Stored::OverBudget);
            }
        } else if let FragmentLayout::Known(buffer) = &self.layout
            && bytes.len() > buffer.fragment_len
        {
            return Err(ChannelReceiveError::InvalidFinalFragmentLen {
                actual: bytes.len(),
                max: buffer.fragment_len,
            });
        }

        match &mut self.layout {
            FragmentLayout::Known(buffer) => {
                if !buffer.mark_received(fragment_index) {
                    return Ok(Stored::Pending);
                }
                buffer.write(fragment_index, is_last_fragment, &bytes);
                self.received_fragments += 1;
                if self.received_fragments == self.fragments {
                    trace!("Received all fragments!");
                    let payload = core::mem::take(&mut buffer.bytes);
                    return Ok(Stored::Complete {
                        tick: self.tick,
                        bytes: payload.into(),
                    });
                }
            }
            FragmentLayout::Unknown { final_fragment } => {
                if final_fragment.is_some() {
                    return Ok(Stored::Pending);
                }
                // The other fragments hold at least a byte each before it.
                let min_len = self.fragments - 1 + bytes.len();
                if min_len > max_message_len {
                    return Err(ChannelReceiveError::MessageTooLarge {
                        min_len,
                        max_message_len,
                    });
                }
                // The only fragment is also the whole message.
                if self.fragments == 1 {
                    return Ok(Stored::Complete {
                        tick: self.tick,
                        bytes,
                    });
                }
                if !reserve(budget, cx.reservation, bytes.len()) {
                    return Ok(Stored::OverBudget);
                }
                self.charged = bytes.len();
                *final_fragment = Some(bytes);
                // The final fragment alone of several completes nothing: the layout is known
                // once a non-final one arrives.
                self.received_fragments += 1;
            }
        }
        Ok(Stored::Pending)
    }

    /// Checks a non-final fragment's length against the message's fragment size, or makes it the
    /// message's fragment size if it is the first non-final fragment, and returns false when the
    /// budget has no room for the buffer that size needs.
    fn learn_fragment_len(
        &mut self,
        fragment_len: usize,
        cx: &FragmentCx,
        budget: &mut ReceiveBudget,
    ) -> Result<bool> {
        let max_message_len = cx.max_message_len;
        let final_fragment = match &mut self.layout {
            FragmentLayout::Known(buffer) => {
                return if fragment_len == buffer.fragment_len {
                    Ok(true)
                } else {
                    Err(ChannelReceiveError::InvalidNonFinalFragmentLen {
                        actual: fragment_len,
                        expected: buffer.fragment_len,
                    })
                };
            }
            FragmentLayout::Unknown { final_fragment } => final_fragment,
        };
        if fragment_len == 0 {
            return Err(ChannelReceiveError::EmptyNonFinalFragment);
        }
        if let Some(final_fragment) = final_fragment
            && final_fragment.len() > fragment_len
        {
            return Err(ChannelReceiveError::InvalidFinalFragmentLen {
                actual: final_fragment.len(),
                max: fragment_len,
            });
        }
        // The smallest message these fragments describe: every non-final fragment full, and a
        // final fragment of one byte, or of its own length once it arrived.
        let final_len = final_fragment.as_ref().map_or(1, Bytes::len);
        let min_len = (self.fragments - 1)
            .checked_mul(fragment_len)
            .and_then(|len| len.checked_add(final_len))
            .filter(|&len| len <= max_message_len)
            .ok_or_else(|| ChannelReceiveError::MessageTooLarge {
                min_len: (self.fragments - 1)
                    .saturating_mul(fragment_len)
                    .saturating_add(final_len),
                max_message_len,
            })?;
        // At most `max_message_len + fragment_len - 1` bytes, by the check above.
        let capacity = min_len - final_len + fragment_len;
        // A final fragment already charged its own length, which the buffer takes over.
        if !reserve(
            budget,
            cx.reservation,
            capacity.saturating_sub(self.charged),
        ) {
            return Ok(false);
        }
        self.charged = self.charged.max(capacity);

        let mut buffer = ReassemblyBuffer {
            fragment_len,
            bytes: vec![0; capacity],
            received: vec![0; self.fragments.div_ceil(64)],
        };
        if let Some(final_fragment) = final_fragment.take() {
            buffer.mark_received(self.fragments - 1);
            buffer.write(self.fragments - 1, true, &final_fragment);
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

    /// `fragment` must hold `fragment_len` bytes, or at most that many if it is the final one.
    fn write(&mut self, fragment_index: usize, is_last_fragment: bool, fragment: &[u8]) {
        // Cannot overflow: the buffer holds `fragments * fragment_len` bytes until the final
        // fragment shortens it, and every non-final fragment lies before the final one.
        let start = fragment_index * self.fragment_len;
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
    setup: CompressionSetup,
) -> Result<Bytes> {
    match compression {
        FragmentCompression::None => Ok(payload),
        FragmentCompression::Lz4 => {
            let setup = setup_for_fragment_compression(compression, setup)?;
            let mut out = Writer::with_capacity(0);
            decompress_payload(payload.as_ref(), setup, &mut out)
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
fn setup_for_fragment_compression(
    compression: FragmentCompression,
    setup: CompressionSetup,
) -> Result<CompressionSetup> {
    match compression {
        FragmentCompression::None => Ok(setup),
        FragmentCompression::Lz4 => {
            #[cfg(feature = "compression_lz4")]
            {
                if setup.algorithm == Some(CompressionAlgorithm::Lz4) {
                    Ok(setup)
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
