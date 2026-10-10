use postcard::ser_flavors::Flavor;

/// A postcard output that writes nothing: it compares each byte of an encoding with the bytes a
/// decode read, and fails at the first that differs or runs past them. Its output is how many
/// matched.
#[derive(Debug)]
pub(crate) struct Compare<'a> {
    read: &'a [u8],
    at: usize,
}

impl<'a> Compare<'a> {
    pub(crate) const fn new(read: &'a [u8]) -> Compare<'a> {
        Compare { read, at: 0 }
    }
}

impl Flavor for Compare<'_> {
    type Output = usize;

    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        self.try_extend(&[byte])
    }

    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        let end = self.at + bytes.len();
        if self.read.get(self.at..end) == Some(bytes) {
            self.at = end;
            Ok(())
        } else {
            Err(postcard::Error::SerializeBufferFull)
        }
    }

    fn finalize(self) -> postcard::Result<usize> {
        Ok(self.at)
    }
}
