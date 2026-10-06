/// A value decoded off the front of some bytes, and the bytes after it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Decoded<'a, T> {
    pub(crate) value: T,
    pub(crate) rest: &'a [u8],
}
