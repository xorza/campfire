use std::fmt;

/// Where an encoding goes in batches: a hash, which takes a value's bytes and keeps none, or
/// bytes.
pub trait Sink: fmt::Debug {
    fn put(&mut self, bytes: &[u8]);
}

impl Sink for Vec<u8> {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}
