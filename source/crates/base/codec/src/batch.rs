use arrayvec::ArrayVec;
use postcard::ser_flavors::Flavor;

use crate::sink::Sink;

/// Postcard writes a byte at a time; batching them spares the sink a call per byte.
const BUFFER: usize = 64;

/// A postcard output that feeds a sink in batches.
#[derive(Debug)]
pub(crate) struct Batch<'a> {
    sink: &'a mut dyn Sink,
    buffer: ArrayVec<u8, BUFFER>,
}

impl<'a> Batch<'a> {
    pub(crate) const fn new(sink: &'a mut dyn Sink) -> Self {
        Batch {
            sink,
            buffer: ArrayVec::new_const(),
        }
    }
}

impl Flavor for Batch<'_> {
    type Output = ();

    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        if self.buffer.is_full() {
            self.sink.put(&self.buffer);
            self.buffer.clear();
        }
        self.buffer.push(byte);
        Ok(())
    }

    /// Bytes that fit join the batch; others go after it, into the emptied batch when they fit
    /// it, else to the sink whole.
    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        if self.buffer.try_extend_from_slice(bytes).is_err() {
            self.sink.put(&self.buffer);
            self.buffer.clear();
            if self.buffer.try_extend_from_slice(bytes).is_err() {
                self.sink.put(bytes);
            }
        }
        Ok(())
    }

    fn finalize(self) -> postcard::Result<()> {
        if !self.buffer.is_empty() {
            self.sink.put(&self.buffer);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::binary::Binary;
    use crate::sink::Sink;

    /// A sink that keeps the length of each batch it is given.
    #[derive(Debug, Default)]
    struct Batches(Vec<usize>);

    impl Sink for Batches {
        fn put(&mut self, bytes: &[u8]) {
            self.0.push(bytes.len());
        }
    }

    #[test]
    fn a_piece_that_fits_an_emptied_batch_joins_it() {
        // Three strings, each a length byte and its text: 1 + 60 bytes fill 61 of the 64; 1 more
        // is 62, and the 10 that do not fit go after the batch, into the emptied one; then 1 + 100,
        // the 100 past any batch, go out after the 11 held, whole. 173 bytes in three calls.
        let value = ("a".repeat(60), "b".repeat(10), "c".repeat(100));
        let mut batches = Batches::default();
        Binary::encode_to(&value, &mut batches);
        assert_eq!(batches.0, [62, 11, 100]);
        let mut bytes = Vec::new();
        Binary::encode_to(&value, &mut bytes);
        assert_eq!(bytes.len(), 173);
        // Nothing held at the end puts nothing.
        let mut none = Batches::default();
        Binary::encode_to(&"c".repeat(100), &mut none);
        assert_eq!(none.0, [1, 100]);
    }
}
