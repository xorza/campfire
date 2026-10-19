/// A variable-length integer of RFC 9000: a value of at most 2^62 - 1, written in network byte
/// order in 1, 2, 4 or 8 bytes, the top two bits of the first byte the base-2 logarithm of the
/// length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VarInt(u64);

impl VarInt {
    pub const MAX: Self = Self((1 << 62) - 1);

    /// `value`, which is at most [`MAX`](Self::MAX): every caller writes a length or an id, far
    /// below it.
    pub const fn new(value: u64) -> Self {
        debug_assert!(value <= Self::MAX.0, "a varint holds at most 2^62 - 1");
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The bytes its encoding takes, the fewest that hold it.
    pub const fn encoded_len(self) -> usize {
        if self.0 <= 63 {
            1
        } else if self.0 <= 16383 {
            2
        } else if self.0 <= 1_073_741_823 {
            4
        } else {
            8
        }
    }
}

/// The bytes of the encoding whose first byte is `first`.
pub(crate) const fn encoded_len_of(first: u8) -> usize {
    1 << (first >> 6)
}

#[cfg(test)]
mod tests;
