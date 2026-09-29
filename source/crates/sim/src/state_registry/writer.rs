use blake3::Hasher;
use postcard::ser_flavors::Flavor;
use serde::Serialize;

/// Postcard writes a byte at a time; batching them keeps BLAKE3 from paying per byte.
const BUFFER: usize = 64;

/// Where encoded state goes: a hasher for the state hash, bytes for a snapshot.
pub(crate) trait Sink {
    fn put(&mut self, bytes: &[u8]);
}

impl Sink for Hasher {
    fn put(&mut self, bytes: &[u8]) {
        self.update(bytes);
    }
}

impl Sink for Vec<u8> {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

/// Writes the postcard encoding of `value` into `sink`.
pub(crate) fn write<T: Serialize>(sink: &mut dyn Sink, value: &T) {
    postcard::serialize_with_flavor(
        value,
        Writer {
            sink,
            buffer: [0; BUFFER],
            len: 0,
        },
    )
    .expect("postcard into a sink cannot fail");
}

/// A postcard output that feeds a sink in batches.
struct Writer<'a> {
    sink: &'a mut dyn Sink,
    buffer: [u8; BUFFER],
    len: usize,
}

impl Flavor for Writer<'_> {
    type Output = ();

    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        if self.len == BUFFER {
            self.sink.put(&self.buffer);
            self.len = 0;
        }
        self.buffer[self.len] = byte;
        self.len += 1;
        Ok(())
    }

    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        if self.len + bytes.len() <= BUFFER {
            self.buffer[self.len..self.len + bytes.len()].copy_from_slice(bytes);
            self.len += bytes.len();
        } else {
            self.sink.put(&self.buffer[..self.len]);
            self.len = 0;
            self.sink.put(bytes);
        }
        Ok(())
    }

    fn finalize(self) -> postcard::Result<()> {
        self.sink.put(&self.buffer[..self.len]);
        Ok(())
    }
}
