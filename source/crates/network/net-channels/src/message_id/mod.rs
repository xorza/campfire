use campfire_net_bytes::{Reader, SerializationError, ToBytes, WriteInteger};

/// The id a reliable or sequenced channel gives each message it sends, which wraps at 2^16. It has
/// no order: circular sequence order is meaningful only beside a nearby anchor, which `a - b`, the
/// signed wrapping distance from `b` to `a`, measures while the two are less than half the space
/// apart.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Default)]
pub struct MessageId(pub u16);

impl ToBytes for MessageId {
    fn bytes_len(&self) -> usize {
        2
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_u16(self.0);
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError> {
        Ok(Self(buffer.read_u16()?))
    }
}

impl From<u16> for MessageId {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl core::ops::Sub for MessageId {
    type Output = i16;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0.wrapping_sub(rhs.0).cast_signed()
    }
}

impl core::ops::Sub<u16> for MessageId {
    type Output = Self;

    fn sub(self, rhs: u16) -> Self::Output {
        Self(self.0.wrapping_sub(rhs))
    }
}

impl core::ops::AddAssign<u16> for MessageId {
    fn add_assign(&mut self, rhs: u16) {
        self.0 = self.0.wrapping_add(rhs);
    }
}

impl core::ops::Add<i16> for MessageId {
    type Output = Self;

    fn add(self, rhs: i16) -> Self::Output {
        Self(self.0.wrapping_add_signed(rhs))
    }
}

#[cfg(test)]
mod tests;
