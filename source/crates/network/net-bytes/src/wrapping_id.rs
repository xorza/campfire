//! `u16` identifiers with wrapping sequence-number arithmetic, and their wire form.

/// Defines a `u16` sequence identifier that wraps around 2^16, with its two-byte wire form.
///
/// The type has no order: circular sequence order is meaningful only relative to a nearby
/// anchor, which `a - b`, the signed wrapping distance from `b` to `a`, measures.
#[macro_export]
macro_rules! wrapping_id {
    ($struct_name:ident) => {
        /// A wrapping sequence identifier.
        ///
        /// This type intentionally does not implement [`Ord`]: circular sequence order is
        /// meaningful only relative to a nearby anchor and cannot satisfy a global ordering
        /// contract. `a - b` is the signed wrapping distance from `b` to `a`, meaningful while
        /// the two are less than half the `u16` space apart.
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Default)]
        pub struct $struct_name(pub u16);

        impl $crate::ToBytes for $struct_name {
            fn bytes_len(&self) -> usize {
                2
            }

            fn to_bytes(&self, buffer: &mut impl $crate::WriteInteger) {
                buffer.write_u16(self.0);
            }

            fn from_bytes(buffer: &mut $crate::Reader) -> Result<Self, $crate::SerializationError>
            where
                Self: Sized,
            {
                Ok(Self(buffer.read_u16()?))
            }
        }

        impl From<u16> for $struct_name {
            fn from(value: u16) -> Self {
                Self(value)
            }
        }

        impl ::core::ops::Sub for $struct_name {
            type Output = i16;

            fn sub(self, rhs: Self) -> Self::Output {
                self.0.wrapping_sub(rhs.0).cast_signed()
            }
        }

        impl ::core::ops::Sub<u16> for $struct_name {
            type Output = Self;

            fn sub(self, rhs: u16) -> Self::Output {
                Self(self.0.wrapping_sub(rhs))
            }
        }

        impl ::core::ops::AddAssign<u16> for $struct_name {
            fn add_assign(&mut self, rhs: u16) {
                self.0 = self.0.wrapping_add(rhs);
            }
        }

        impl ::core::ops::Add<i16> for $struct_name {
            type Output = Self;

            fn add(self, rhs: i16) -> Self::Output {
                Self(self.0.wrapping_add_signed(rhs))
            }
        }
    };
}

#[cfg(test)]
#[expect(unreachable_pub, reason = "the macro defines a public id type")]
mod tests {
    use crate::reader::Reader;
    use crate::to_bytes::ToBytes;
    use crate::writer::Writer;

    crate::wrapping_id!(TestId);

    /// The distance from 0 to 2 is 2 and back is -2, across the wrap from 65,535 to 1 too; half
    /// the space less one, 32,767, is still a forward distance.
    #[test]
    fn the_distance_wraps() {
        assert_eq!(TestId(12) - TestId(10), 2);
        assert_eq!(TestId(10) - TestId(12), -2);
        assert_eq!(TestId(1) - TestId(u16::MAX), 2);
        assert_eq!(TestId(u16::MAX) - TestId(1), -2);
        assert_eq!(TestId(i16::MAX.cast_unsigned()) - TestId(0), i16::MAX);
        assert_eq!(TestId(u16::MAX) + 2, TestId(1));
    }

    #[test]
    fn an_id_takes_two_bytes() {
        let mut writer = Writer::default();
        TestId(513).to_bytes(&mut writer);
        assert_eq!(TestId(513).bytes_len(), 2);
        let mut reader = Reader::from(writer.take_written());
        assert_eq!(TestId::from_bytes(&mut reader).unwrap(), TestId(513));
    }
}
