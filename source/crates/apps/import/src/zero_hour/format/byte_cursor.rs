use crate::zero_hour::format::error::CutShort;

/// Bytes of the original and how far a read has taken them: little-endian fields, each refused
/// when it runs past the end, as a map's `DataChunkInput` and a model's chunks read them.
#[derive(Debug, Clone)]
pub(crate) struct ByteCursor<'a> {
    rest: &'a [u8],
}

impl<'a> ByteCursor<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        ByteCursor { rest: bytes }
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    /// The bytes no read took yet.
    pub(crate) const fn rest(&self) -> &'a [u8] {
        self.rest
    }

    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8], CutShort> {
        let (taken, rest) = self.rest.split_at_checked(len).ok_or(CutShort)?;
        self.rest = rest;
        Ok(taken)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], CutShort> {
        let (taken, rest) = self.rest.split_first_chunk::<N>().ok_or(CutShort)?;
        self.rest = rest;
        Ok(*taken)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, CutShort> {
        Ok(u8::from_le_bytes(self.array()?))
    }

    pub(crate) fn u16(&mut self) -> Result<u16, CutShort> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub(crate) fn i32(&mut self) -> Result<i32, CutShort> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, CutShort> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub(crate) fn f32(&mut self) -> Result<f32, CutShort> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    /// `count` records of `N` bytes.
    pub(crate) fn records<const N: usize>(
        &mut self,
        count: usize,
    ) -> Result<&'a [[u8; N]], CutShort> {
        let bytes = self.take(count.checked_mul(N).ok_or(CutShort)?)?;
        Ok(bytes.as_chunks::<N>().0)
    }

    /// What is left as whole records of `N` bytes.
    pub(crate) fn rest_records<const N: usize>(&mut self) -> Result<&'a [[u8; N]], CutShort> {
        if !self.rest.len().is_multiple_of(N) {
            return Err(CutShort);
        }
        self.records(self.rest.len() / N)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_read_little_endian_and_stop_at_the_end() {
        let bytes = [
            &[7][..],
            &[0x34, 0x12],
            &(-2_i32).to_le_bytes(),
            &0xDEAD_BEEF_u32.to_le_bytes(),
            &1.5_f32.to_le_bytes(),
            &[1, 2, 3, 4, 5, 6],
        ]
        .concat();
        let mut cursor = ByteCursor::new(&bytes);
        assert_eq!(cursor.u8(), Ok(7));
        assert_eq!(cursor.u16(), Ok(0x1234));
        assert_eq!(cursor.i32(), Ok(-2));
        assert_eq!(cursor.u32(), Ok(0xDEAD_BEEF));
        assert_eq!(cursor.f32(), Ok(1.5));
        assert_eq!(cursor.clone().records::<2>(2), Ok(&[[1, 2], [3, 4]][..]));
        assert_eq!(cursor.clone().rest_records::<4>(), Err(CutShort));
        assert_eq!(cursor.clone().records::<4>(2), Err(CutShort));
        assert_eq!(cursor.rest_records::<3>(), Ok(&[[1, 2, 3], [4, 5, 6]][..]));
        assert!(cursor.is_empty());
        assert_eq!(cursor.u8(), Err(CutShort));
        assert_eq!(ByteCursor::new(&[1]).u16(), Err(CutShort));
        assert_eq!(ByteCursor::new(&[1]).take(2), Err(CutShort));
    }
}
