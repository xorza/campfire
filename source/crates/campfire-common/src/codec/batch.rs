use arrayvec::ArrayVec;
use postcard::ser_flavors::Flavor;

use crate::codec::sink::Sink;

/// Postcard writes a byte at a time; batching them spares the sink a call per byte.
const BUFFER: usize = 64;

/// A postcard output that feeds a sink in batches.
#[derive(Debug)]
pub(crate) struct Batch<'a> {
    sink: &'a mut dyn Sink,
    buffer: ArrayVec<u8, BUFFER>,
}

impl<'a> Batch<'a> {
    pub(crate) const fn new(sink: &'a mut dyn Sink) -> Batch<'a> {
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

    /// Bytes that fit join the batch; more go to the sink whole, after the batch.
    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        if self.buffer.try_extend_from_slice(bytes).is_err() {
            self.sink.put(&self.buffer);
            self.buffer.clear();
            self.sink.put(bytes);
        }
        Ok(())
    }

    fn finalize(self) -> postcard::Result<()> {
        self.sink.put(&self.buffer);
        Ok(())
    }
}
