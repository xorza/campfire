use core::ops::{Add, AddAssign, Deref as CoreDeref, Sub};

#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use campfire_net_bytes::{ReadInteger, Reader};
use serde::{Deserialize, Serialize};

/// Monotonically increasing simulation tick shared by the server and clients (13-net-stack/time.md,
/// The tick family saturates).
///
/// Unlike packet and message sequence IDs, ticks are numerically ordered and do not wrap. At
/// 60 Hz, exhausting the `u32` range takes more than two years of continuous runtime. Arithmetic
/// that produces another [`NetTick`] saturates at `0` or [`u32::MAX`] instead of wrapping; signed
/// tick differences saturate to the `i32` range.
#[repr(transparent)]
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd, Default,
)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
pub struct NetTick(pub u32);

impl ToBytes for NetTick {
    fn bytes_len(&self) -> usize {
        size_of::<u32>()
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_u32(self.0);
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError> {
        Ok(Self(buffer.read_u32()?))
    }
}

impl From<u32> for NetTick {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl CoreDeref for NetTick {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Sub for NetTick {
    type Output = i32;

    fn sub(self, rhs: Self) -> Self::Output {
        let difference = i64::from(self.0) - i64::from(rhs.0);
        i32::try_from(difference).unwrap_or(if difference < 0 { i32::MIN } else { i32::MAX })
    }
}

impl Sub<u32> for NetTick {
    type Output = Self;

    fn sub(self, rhs: u32) -> Self::Output {
        Self(self.0.saturating_sub(rhs))
    }
}

impl AddAssign<u32> for NetTick {
    fn add_assign(&mut self, rhs: u32) {
        self.0 = self.0.saturating_add(rhs);
    }
}

impl Add<i32> for NetTick {
    type Output = Self;

    fn add(self, rhs: i32) -> Self::Output {
        Self(self.0.saturating_add_signed(rhs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tick_orders_by_its_number_and_saturates() {
        assert!(NetTick(u32::MAX) > NetTick(0));
        assert_eq!(NetTick(0) - 1, NetTick(0));
        assert_eq!(NetTick(u32::MAX) + 1, NetTick(u32::MAX));
        assert_eq!(NetTick(10) + (-20), NetTick(0));
        assert_eq!(NetTick(u32::MAX) - NetTick(0), i32::MAX);
        assert_eq!(NetTick(0) - NetTick(u32::MAX), i32::MIN);
    }
}
