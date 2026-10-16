use std::iter;

use serde::{Deserialize, Serialize};

/// 256 bits, by index: the storage of the sets of tags and of teams, which a rollback copies
/// without an allocation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Bits256([u64; Self::WORDS]);

impl Bits256 {
    pub(crate) const BITS: usize = 256;
    const WORDS: usize = Self::BITS / 64;
    pub const NONE: Self = Self([0; Self::WORDS]);
    pub const ALL: Self = Self([u64::MAX; Self::WORDS]);

    /// The bits with bit `index`, below `BITS`, set.
    #[must_use]
    pub const fn with(mut self, index: usize) -> Self {
        self.0[index / 64] |= 1 << (index % 64);
        self
    }

    /// The bits with bit `index`, below `BITS`, clear.
    #[must_use]
    pub const fn without(mut self, index: usize) -> Self {
        self.0[index / 64] &= !(1 << (index % 64));
        self
    }

    pub const fn contains(self, index: usize) -> bool {
        self.0[index / 64] & (1 << (index % 64)) != 0
    }

    /// The bits set in either.
    #[must_use]
    pub(crate) const fn union(mut self, other: Self) -> Self {
        let mut word = 0;
        while word < Self::WORDS {
            self.0[word] |= other.0[word];
            word += 1;
        }
        self
    }

    /// Whether the two set a bit in common.
    pub const fn meets(self, other: Self) -> bool {
        let mut word = 0;
        while word < Self::WORDS {
            if self.0[word] & other.0[word] != 0 {
                return true;
            }
            word += 1;
        }
        false
    }

    /// Whether it sets every bit `other` sets.
    pub(crate) const fn covers(self, other: Self) -> bool {
        let mut word = 0;
        while word < Self::WORDS {
            if other.0[word] & !self.0[word] != 0 {
                return false;
            }
            word += 1;
        }
        true
    }

    /// The indices of its set bits, in order.
    pub fn iter(self) -> impl Iterator<Item = usize> {
        self.0.into_iter().enumerate().flat_map(|(word, mut bits)| {
            iter::from_fn(move || {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits.wrapping_sub(1);
                (bit < 64).then_some(word * 64 + bit)
            })
        })
    }
}
