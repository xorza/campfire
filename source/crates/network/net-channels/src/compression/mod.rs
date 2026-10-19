//! Packet payload compression helpers.
//!
//! Compression applies to a packet's body, after its tick, which stays uncompressed: the receiver
//! reads the tick before it decompresses the body that the packet's kind names compressed.

#[cfg(feature = "compression_lz4")]
use lz4_flex::block;

#[cfg(feature = "compression_lz4")]
use crate::compression_algorithm::CompressionAlgorithm;
use crate::compression_scratch::CompressionScratch;
use crate::compression_setup::CompressionSetup;
use crate::packet_error::PacketError;
use campfire_net_bytes::Writer;

/// What a compression attempt did, on a packet or on a fragmented message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompressionOutcome {
    Disabled,
    TooSmall {
        payload_len: usize,
    },
    TooLargeForDecompressionLimit {
        payload_len: usize,
        limit: usize,
    },
    NotSmaller {
        original_len: usize,
        compressed_len: usize,
    },
    Compressed {
        original_len: usize,
        compressed_len: usize,
    },
}

/// What a packet's body is compressed against: the algorithm and its limits, and the bytes its
/// compressed form must fit in.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PacketCompression {
    pub(crate) setup: CompressionSetup,
    pub(crate) room: usize,
}

impl PacketCompression {
    /// Compresses a packet's `body` into `output`, and keeps it when it is smaller and fits the
    /// room; the outcome's lengths are the body's.
    ///
    /// The builder tries it while it packs messages too, so that a body past the room whose
    /// compressed form fits still goes.
    pub(crate) fn evaluate(
        self,
        body: &[u8],
        scratch: &mut CompressionScratch,
        output: &mut Vec<u8>,
    ) -> Result<CompressionOutcome, PacketError> {
        let Some(algorithm) = self.setup.algorithm else {
            return Ok(CompressionOutcome::Disabled);
        };
        let payload_len = body.len();
        if payload_len == 0 || payload_len < self.setup.min_payload_len {
            return Ok(CompressionOutcome::TooSmall { payload_len });
        }
        if payload_len > self.setup.max_decompressed_payload_len {
            return Ok(CompressionOutcome::TooLargeForDecompressionLimit {
                payload_len,
                limit: self.setup.max_decompressed_payload_len,
            });
        }
        let compressed_len = scratch.compress_into(body, algorithm, output)?.len();
        if compressed_len >= payload_len || compressed_len > self.room {
            return Ok(CompressionOutcome::NotSmaller {
                original_len: payload_len,
                compressed_len,
            });
        }
        Ok(CompressionOutcome::Compressed {
            original_len: payload_len,
            compressed_len,
        })
    }
}

/// Compresses a message about to be fragmented. On [`CompressionOutcome::Compressed`], the
/// compressed bytes are the first `compressed_len` of `scratch`'s output.
pub(crate) fn compress_fragment(
    payload: &[u8],
    setup: CompressionSetup,
    scratch: &mut CompressionScratch,
) -> Result<CompressionOutcome, PacketError> {
    let Some(algorithm) = setup.algorithm else {
        return Ok(CompressionOutcome::Disabled);
    };

    let payload_len = payload.len();
    if payload_len < setup.min_payload_len {
        return Ok(CompressionOutcome::TooSmall { payload_len });
    }
    if payload_len > setup.max_decompressed_payload_len {
        return Ok(CompressionOutcome::TooLargeForDecompressionLimit {
            payload_len,
            limit: setup.max_decompressed_payload_len,
        });
    }

    let compressed_len = scratch.compress(payload, algorithm)?.len();
    if compressed_len >= payload_len {
        return Ok(CompressionOutcome::NotSmaller {
            original_len: payload_len,
            compressed_len,
        });
    }
    Ok(CompressionOutcome::Compressed {
        original_len: payload_len,
        compressed_len,
    })
}

/// Appends the decompressed `compressed_payload` to `out`.
#[cfg_attr(
    not(feature = "compression_lz4"),
    expect(
        unused_variables,
        clippy::missing_const_for_fn,
        clippy::needless_pass_by_ref_mut,
        reason = "no compression backend is compiled in"
    )
)]
pub(crate) fn decompress_payload(
    compressed_payload: &[u8],
    setup: CompressionSetup,
    out: &mut Writer,
) -> Result<(), PacketError> {
    let Some(algorithm) = setup.algorithm else {
        return Err(PacketError::UnsupportedCompression);
    };

    match algorithm {
        #[cfg(feature = "compression_lz4")]
        CompressionAlgorithm::Lz4 => decompress_lz4(compressed_payload, setup, out),
    }
}

#[cfg(feature = "compression_lz4")]
fn decompress_lz4(
    compressed_payload: &[u8],
    setup: CompressionSetup,
    out: &mut Writer,
) -> Result<(), PacketError> {
    let (uncompressed_len, compressed_payload) = block::uncompressed_size(compressed_payload)
        .map_err(|_| PacketError::DecompressionFailed)?;

    if uncompressed_len > setup.max_decompressed_payload_len {
        return Err(PacketError::DecompressedPayloadTooLarge {
            actual: uncompressed_len,
            limit: setup.max_decompressed_payload_len,
        });
    }

    out.fill(
        uncompressed_len,
        |decompressed| match block::decompress_into(compressed_payload, decompressed) {
            Ok(decoded_len) if decoded_len == uncompressed_len => Ok(()),
            _ => Err(PacketError::DecompressionFailed),
        },
    )
}

#[cfg(test)]
pub(crate) mod tests;
