use campfire_math::Num;
use serde::{Deserialize, Serialize};

use crate::body_bound::MAX_BOUND;
use crate::radius::error::NotRadius;

pub(crate) mod error;

/// A circle's radius: from 0, a point's, to `MAX_BOUND`, the widest body's. It writes as its
/// number, and a number past these does not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "Num", into = "Num")]
pub struct Radius(Num);

impl Radius {
    pub const ZERO: Self = Self(Num::ZERO);

    /// `radius`; `None` below 0 or past `MAX_BOUND`.
    pub const fn new(radius: Num) -> Option<Self> {
        if radius.to_bits() < 0 || radius.to_bits() > MAX_BOUND.to_bits() {
            return None;
        }
        Some(Self(radius))
    }

    pub const fn get(self) -> Num {
        self.0
    }
}

impl TryFrom<Num> for Radius {
    type Error = NotRadius;

    fn try_from(radius: Num) -> Result<Self, NotRadius> {
        Self::new(radius).ok_or(NotRadius(radius))
    }
}

impl From<Radius> for Num {
    fn from(radius: Radius) -> Self {
        radius.0
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;
    use campfire_math::Num;

    use crate::body_bound::MAX_BOUND;
    use crate::radius::Radius;
    use crate::radius::error::NotRadius;

    #[test]
    fn a_radius_lies_from_0_to_the_widest_body_and_reads_back_as_its_number() {
        let below = Num::ZERO - Num::EPSILON;
        let past = MAX_BOUND + Num::EPSILON;
        assert_eq!(Radius::new(Num::ZERO), Some(Radius::ZERO));
        assert_eq!(Radius::new(MAX_BOUND).map(Radius::get), Some(MAX_BOUND));
        assert_eq!(Radius::new(below), None);
        assert_eq!(Radius::new(past), None);
        assert_eq!(Radius::try_from(past), Err(NotRadius(past)));
        let two = Radius::new(Num::int(2)).unwrap();
        let bytes = Binary::encode(&two);
        assert_eq!(bytes, Binary::encode(&Num::int(2)));
        assert_eq!(Binary::decode::<Radius>(&bytes).unwrap(), two);
        assert!(Binary::decode::<Radius>(&Binary::encode(&past)).is_err());
    }
}
