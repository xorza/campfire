/// The name of a random stream, which keeps its draws apart from every other stream's of the
/// same entity and tick. Each stream is a constant its owner declares, checked as it is built, so
/// no stream is text made while a match runs; its bytes are what the RNG hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RngStream(&'static str);

impl RngStream {
    /// The longest name.
    pub(crate) const MAX_LEN: usize = 64;

    /// The stream `name`: from 1 to 64 bytes of lowercase ASCII letters, digits, `_` and `.`.
    /// Another name is a bug of the code that names it, and fails to compile in a constant.
    pub const fn new(name: &'static str) -> RngStream {
        let bytes = name.as_bytes();
        assert!(
            !bytes.is_empty() && bytes.len() <= Self::MAX_LEN,
            "an RNG stream's name has 1 to 64 bytes"
        );
        let mut at = 0;
        while at < bytes.len() {
            let byte = bytes[at];
            assert!(
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'.',
                "an RNG stream's name is lowercase ASCII letters, digits, `_` and `.`"
            );
            at += 1;
        }
        RngStream(name)
    }

    pub(crate) const fn as_bytes(self) -> &'static [u8] {
        self.0.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use std::panic;

    use super::*;

    #[test]
    fn a_stream_takes_only_a_name_of_its_alphabet() {
        assert_eq!(RngStream::new("combat.roll").as_bytes(), b"combat.roll");
        let long = "a".repeat(64).leak();
        assert_eq!(RngStream::new(long).as_bytes().len(), 64);
        let refused = ["", "Combat", "combat roll", "ü", &*"a".repeat(65).leak()];
        for name in refused {
            let built = panic::catch_unwind(|| RngStream::new(name));
            assert!(built.is_err(), "{name:?}");
        }
    }
}
