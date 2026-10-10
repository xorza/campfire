use blake3::{Hasher, OutputReader};
use campfire_common::SegmentSeed;

use crate::num::Num;
use crate::rng::rng_stream::RngStream;

#[cfg(feature = "bench")]
pub(crate) mod bench;
pub(crate) mod rng_opener;
pub(crate) mod rng_source;
pub(crate) mod rng_stream;
#[cfg(any(test, feature = "internals"))]
pub(crate) mod split_mix64;

/// Starts every message, so no other use of a segment seed can produce the same output.
const DOMAIN: &[u8] = b"campfire/rng/v1";
/// BLAKE3 computes its output a whole 64-byte block at a time, and `OutputReader::fill` redoes
/// that for every partial read, so `Rng` reads whole blocks and serves words from them.
const BLOCK_LEN: usize = 64;
const WORD_LEN: usize = 8;
/// The longest message: the domain, the stream's length and its longest name, the entity and the
/// tick.
const MAX_MESSAGE_LEN: usize = DOMAIN.len() + 4 + RngStream::MAX_LEN + 8 + 8;

/// The random sequence of one (stream, entity, tick): keyed BLAKE3 extended output, so the draws
/// a client sees do not reveal the seed.
#[derive(Debug, Clone)]
pub struct Rng {
    reader: OutputReader,
    block: [u8; BLOCK_LEN],
    used: usize,
}

impl Rng {
    /// The message is `DOMAIN ‖ u32 stream length ‖ stream ‖ u64 entity ‖ u64 tick`, little-endian;
    /// the length prefix keeps two fields from running together.
    pub(crate) fn new(seed: &SegmentSeed, stream: RngStream, entity: u64, tick: u64) -> Rng {
        let stream = stream.as_bytes();
        let stream_len = u32::try_from(stream.len()).expect("a stream's name has at most 64 bytes");
        // The whole message in one update, as each update pays its own checks and copies.
        let mut message = [0; MAX_MESSAGE_LEN];
        let mut at = 0;
        for part in [
            DOMAIN,
            &stream_len.to_le_bytes(),
            stream,
            &entity.to_le_bytes(),
            &tick.to_le_bytes(),
        ] {
            message[at..at + part.len()].copy_from_slice(part);
            at += part.len();
        }
        let mut hasher = Hasher::new_keyed(seed.as_bytes());
        hasher.update(&message[..at]);
        Rng {
            reader: hasher.finalize_xof(),
            block: [0; BLOCK_LEN],
            used: BLOCK_LEN,
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        if self.used == BLOCK_LEN {
            self.reader.fill(&mut self.block);
            self.used = 0;
        }
        let mut word = [0; WORD_LEN];
        word.copy_from_slice(&self.block[self.used..self.used + WORD_LEN]);
        self.used += WORD_LEN;
        u64::from_le_bytes(word)
    }

    /// A uniform integer in `[0, bound)`, by Lemire's multiply-and-reject; `bound` is not zero.
    pub fn below(&mut self, bound: u64) -> u64 {
        debug_assert!(bound != 0, "Rng::below(0)");
        loop {
            if let Some(value) = lemire_step::<64>(self.next_u64(), bound) {
                return value;
            }
        }
    }

    /// True with probability exactly `probability`, clamped to `[0, 1]`. It always takes one
    /// word, so later draws do not depend on the probability.
    pub fn chance(&mut self, probability: Num) -> bool {
        chance_from_word(self.next_u64(), probability)
    }

    /// A uniform number in `[0, 1)`, of a `Num`'s 24 fractional bits: the top 24 bits of one word,
    /// so it is below a probability exactly when `chance` with that probability would be true for
    /// the same word.
    pub fn fraction(&mut self) -> Num {
        fraction_of_word(self.next_u64())
    }

    /// True with probability exactly `numerator / denominator`, for chances too small for the
    /// 24 fractional bits of `Num`; `denominator` is not zero.
    pub fn chance_ratio(&mut self, numerator: u64, denominator: u64) -> bool {
        self.below(denominator) < numerator
    }

    /// A uniform index into `len` items; `len` is not zero.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the index is below len, a usize"
    )]
    pub fn pick(&mut self, len: usize) -> usize {
        self.below(len as u64) as usize
    }
}

/// One draw of Lemire's method with `BITS`-bit words: the value below `bound`, or `None` when
/// this word must be rejected. `bound` is at most 2^`BITS`.
const fn lemire_step<const BITS: u32>(word: u64, bound: u64) -> Option<u64> {
    let product = word as u128 * bound as u128;
    let low = product & ((1 << BITS) - 1);
    if low < bound as u128 {
        // (2^BITS − bound) mod bound words are rejected; only this rare branch pays a division.
        let threshold = ((1 << BITS) - bound as u128) % bound as u128;
        if low < threshold {
            return None;
        }
    }
    #[expect(clippy::cast_possible_truncation, reason = "the result is below bound")]
    Some((product >> BITS) as u64)
}

/// The top 24 bits of `word`, as a number in `[0, 1)`.
const fn fraction_of_word(word: u64) -> Num {
    Num::from_bits((word >> (u64::BITS - Num::FRAC_BITS)).cast_signed())
}

/// Whether `word` falls below the probability: exact for every `Num` in `[0, 1]`, since its 24
/// fractional bits are compared with the word's top 24 bits.
const fn chance_from_word(word: u64, probability: Num) -> bool {
    let threshold = probability.to_bits();
    if threshold <= 0 {
        false
    } else if threshold >= Num::ONE.to_bits() {
        true
    } else {
        word >> (u64::BITS - Num::FRAC_BITS) < threshold.cast_unsigned()
    }
}

#[cfg(test)]
mod tests;
