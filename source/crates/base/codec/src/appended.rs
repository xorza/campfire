use postcard::ser_flavors::Flavor;

/// A postcard output that appends to a `Vec` in place, with no batch and no call through a sink.
#[derive(Debug)]
pub(crate) struct Appended<'a>(pub(crate) &'a mut Vec<u8>);

impl Flavor for Appended<'_> {
    type Output = ();

    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        self.0.push(byte);
        Ok(())
    }

    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        self.0.extend_from_slice(bytes);
        Ok(())
    }

    fn finalize(self) -> postcard::Result<()> {
        Ok(())
    }
}
