/// The largest value a variable-length integer holds, 2^62 - 1.
pub const MAX: u64 = (1 << 62) - 1;

/// Returns how many bytes it would take to encode `v` as a variable-length
/// integer.
///
/// `v` is at most [`MAX`]: every caller writes a length or an id, far below it.
pub const fn len(v: u64) -> usize {
    debug_assert!(v <= MAX, "a varint holds at most 2^62 - 1");
    if v <= 63 {
        1
    } else if v <= 16383 {
        2
    } else if v <= 1_073_741_823 {
        4
    } else {
        8
    }
}

/// Returns how long the variable-length integer is, given its first byte: its top two bits are
/// the base-2 logarithm of the length.
pub const fn parse_len(first: u8) -> usize {
    1 << (first >> 6)
}

#[cfg(test)]
mod tests;
