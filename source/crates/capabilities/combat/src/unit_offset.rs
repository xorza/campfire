use campfire_geometry::MAX_BOUND;
use campfire_math::{Num, Vec3};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A point in its unit's frame, as its model stands at rest: `x` along its facing, `y` up from
/// its place and `z` to its right, each within `MAX_BOUND` meters, as data writes it, `[x, y, z]`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnitOffset(Vec3);

impl UnitOffset {
    /// Its unit's place.
    pub const ORIGIN: Self = Self(Vec3::ZERO);

    /// The point `[x, y, z]`; `None` for a coordinate past `MAX_BOUND` either way.
    pub fn new([x, y, z]: [Num; 3]) -> Option<Self> {
        let bound = -MAX_BOUND..=MAX_BOUND;
        [x, y, z]
            .iter()
            .all(|coordinate| bound.contains(coordinate))
            .then_some(Self(Vec3::new(x, y, z)))
    }

    pub(crate) const fn get(self) -> Vec3 {
        self.0
    }

    pub(crate) fn is_origin(&self) -> bool {
        *self == Self::ORIGIN
    }
}

impl Serialize for UnitOffset {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        [self.0.x, self.0.y, self.0.z].serialize(serializer)
    }
}

/// Data is untrusted, so a coordinate past `MAX_BOUND` fails to read.
impl<'de> Deserialize<'de> for UnitOffset {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let point = <[Num; 3]>::deserialize(deserializer)?;
        Self::new(point).ok_or_else(|| {
            D::Error::custom("a point of a unit's frame lies within 2,048 m of its place")
        })
    }
}
