use crate::zero_hour::format::byte_cursor::ByteCursor;
use crate::zero_hour::format::w3d::error::W3dError;

/// The fields of a chunk's body: its cursor's, and a model's vectors, names and lists.
#[derive(Debug, Clone)]
pub(crate) struct W3dFields<'a> {
    pub(crate) cursor: ByteCursor<'a>,
}

impl<'a> W3dFields<'a> {
    pub(crate) const fn of(body: &'a [u8]) -> Self {
        W3dFields {
            cursor: ByteCursor::new(body),
        }
    }

    pub(crate) fn vector(&mut self) -> Result<[f32; 3], W3dError> {
        Ok([self.cursor.f32()?, self.cursor.f32()?, self.cursor.f32()?])
    }

    /// A fixed field of `len` bytes holding a name up to its first NUL, in lowercase, as W3D
    /// compares names ignoring case.
    pub(crate) fn name(&mut self, len: usize) -> Result<String, W3dError> {
        let bytes = self.cursor.take(len)?;
        let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(len);
        let name = &bytes[..end];
        if !name.is_ascii() {
            return Err(W3dError::NotAscii);
        }
        Ok(String::from_utf8(name.to_ascii_lowercase()).expect("ASCII is UTF-8"))
    }

    /// What is left as `u32`s, which must be whole.
    pub(crate) fn u32s(&mut self) -> Result<Vec<u32>, W3dError> {
        let records = self.records::<4>()?;
        Ok(records.into_iter().map(u32::from_le_bytes).collect())
    }

    /// What is left as whole records of `N` bytes.
    pub(crate) fn records<const N: usize>(&mut self) -> Result<Vec<[u8; N]>, W3dError> {
        Ok(self.cursor.rest_records::<N>()?.to_vec())
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
    use crate::zero_hour::format::error::Short;
    use crate::zero_hour::format::w3d::w3d_fields::internals::name;

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
        assert_eq!(fields.cursor.u16().unwrap(), 0x1234);
        assert_eq!(fields.cursor.u32().unwrap(), 0xDEAD_BEEF);
        assert_eq!(fields.cursor.f32().unwrap(), 1.5);
        assert_eq!(fields.name(8).unwrap(), "mixed");
        assert_eq!(fields.u32s().unwrap(), [7, 9]);
        assert_eq!(fields.cursor.u32(), Err(Short));
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
