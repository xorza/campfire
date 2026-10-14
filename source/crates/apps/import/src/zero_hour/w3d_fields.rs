use crate::zero_hour::error::W3dError;

/// Little-endian fields of a chunk's body, each refused when it runs past the end.
#[derive(Debug, Clone)]
pub(crate) struct W3dFields<'a> {
    rest: &'a [u8],
}

impl<'a> W3dFields<'a> {
    pub(crate) const fn of(body: &'a [u8]) -> W3dFields<'a> {
        W3dFields { rest: body }
    }

    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8], W3dError> {
        let (taken, rest) = self.rest.split_at_checked(len).ok_or(W3dError::Short)?;
        self.rest = rest;
        Ok(taken)
    }

    pub(crate) fn u16(&mut self) -> Result<u16, W3dError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("2 bytes"),
        ))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, W3dError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    pub(crate) fn f32(&mut self) -> Result<f32, W3dError> {
        Ok(f32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    pub(crate) fn vector(&mut self) -> Result<[f32; 3], W3dError> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    /// A fixed field of `len` bytes holding a name up to its first NUL, in lowercase, as W3D
    /// compares names ignoring case.
    pub(crate) fn name(&mut self, len: usize) -> Result<String, W3dError> {
        let bytes = self.take(len)?;
        let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(len);
        let name = &bytes[..end];
        if !name.is_ascii() {
            return Err(W3dError::NotAscii);
        }
        Ok(String::from_utf8(name.to_ascii_lowercase()).expect("ASCII is UTF-8"))
    }

    /// What is left as `u32`s, which must be whole.
    pub(crate) fn u32s(&mut self) -> Result<Vec<u32>, W3dError> {
        Ok(self
            .records::<4>()?
            .into_iter()
            .map(u32::from_le_bytes)
            .collect())
    }

    /// What is left as whole records of `N` bytes.
    pub(crate) fn records<const N: usize>(&mut self) -> Result<Vec<[u8; N]>, W3dError> {
        let (records, rest) = self.take(self.rest.len())?.as_chunks::<N>();
        if rest.is_empty() {
            Ok(records.to_vec())
        } else {
            Err(W3dError::Short)
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {
    /// `name` in a field of `len` bytes, the rest NUL.
    pub(crate) fn name(name: &str, len: usize) -> Vec<u8> {
        assert!(name.len() <= len, "{name} fits {len} bytes");
        let mut field = name.as_bytes().to_vec();
        field.resize(len, 0);
        field
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::w3d_fields::internals::name;

    #[test]
    fn fields_read_little_endian_and_names_up_to_their_nul() {
        let bytes = [
            &[0x34, 0x12][..],
            &0xDEAD_BEEF_u32.to_le_bytes(),
            &1.5_f32.to_le_bytes(),
            &name("MiXeD", 8),
            &[7, 0, 0, 0, 9, 0, 0, 0],
        ]
        .concat();
        let mut fields = W3dFields::of(&bytes);
        assert_eq!(fields.u16().unwrap(), 0x1234);
        assert_eq!(fields.u32().unwrap(), 0xDEAD_BEEF);
        assert_eq!(fields.f32().unwrap(), 1.5);
        assert_eq!(fields.name(8).unwrap(), "mixed");
        assert_eq!(fields.u32s().unwrap(), [7, 9]);
        assert!(matches!(fields.u32(), Err(W3dError::Short)));
        // A name of no NUL fills its field; one past ASCII is refused; a tail not whole is short.
        assert_eq!(W3dFields::of(b"ABCD").name(4).unwrap(), "abcd");
        assert!(matches!(
            W3dFields::of(&[0xE9, 0]).name(2),
            Err(W3dError::NotAscii)
        ));
        assert!(matches!(
            W3dFields::of(&[1, 2, 3]).records::<2>(),
            Err(W3dError::Short)
        ));
        assert!(matches!(
            W3dFields::of(&[1, 2, 3]).u32s(),
            Err(W3dError::Short)
        ));
    }
}
