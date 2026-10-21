use campfire_math::{Num, Rounding, Vec3};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::body_box::BodyBox;

/// A map's bounds: the closed rectangle from `min` to `max` on the ground plane, each `[x, z]`.
/// No unit is ever outside them. A map's `[bounds]` writes them as `min` and `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Bounds {
    min: [Num; 2],
    max: [Num; 2],
}

impl Bounds {
    /// The whole world, within `Vec3::BOUND`: the bounds of a match without a map.
    pub const WORLD: Self = Self {
        min: [Num::from_bits(-Vec3::BOUND.to_bits()); 2],
        max: [Vec3::BOUND; 2],
    };

    /// The bounds from `min` to `max`; `None` unless `min` is below `max` on both axes and both
    /// are within the world's bound.
    pub const fn new(min: [Num; 2], max: [Num; 2]) -> Option<Self> {
        let bound = Vec3::BOUND.to_bits();
        let mut axis = 0;
        while axis < 2 {
            let (low, high) = (min[axis].to_bits(), max[axis].to_bits());
            if low >= high || low < -bound || high > bound {
                return None;
            }
            axis += 1;
        }
        Some(Self { min, max })
    }

    pub const fn min(self) -> [Num; 2] {
        self.min
    }

    pub const fn max(self) -> [Num; 2] {
        self.max
    }

    /// Whether `position` is within the bounds on the ground plane, edges included, and within the
    /// world's bound in height, as every place is.
    pub const fn contains(self, position: Vec3) -> bool {
        self.min[0].to_bits() <= position.x.to_bits()
            && position.x.to_bits() <= self.max[0].to_bits()
            && self.min[1].to_bits() <= position.z.to_bits()
            && position.z.to_bits() <= self.max[1].to_bits()
            && position.y.to_bits().unsigned_abs() <= Vec3::BOUND.to_bits().unsigned_abs()
    }

    /// Whether the box `body` at `position` lies within the bounds on the ground plane, its edges
    /// on theirs among them.
    pub fn holds_box(self, position: Vec3, body: &BodyBox) -> bool {
        let extent = body.extent();
        let ground = [position.x, position.z];
        (0..2).all(|axis| {
            ground[axis]
                .checked_sub(extent[axis])
                .is_some_and(|low| low >= self.min[axis])
                && ground[axis]
                    .checked_add(extent[axis])
                    .is_some_and(|high| high <= self.max[axis])
        })
    }

    /// How far a straight path from `from` along the unit vector `direction` stays within the
    /// bounds on the ground plane, and within the world's bound in height: 0 from a point
    /// outside. Each axis's distance rounds toward the start, so the path that far reaches no
    /// edge's far side. An axis the path barely moves along cannot be the one it leaves by, so a
    /// ratio too large to hold is never the nearest.
    pub fn exit(self, from: Vec3, direction: Vec3) -> Num {
        if !self.contains(from) {
            return Num::ZERO;
        }
        let height = Vec3::BOUND;
        [
            (from.x, direction.x, self.min[0], self.max[0]),
            (from.y, direction.y, -height, height),
            (from.z, direction.z, self.min[1], self.max[1]),
        ]
        .into_iter()
        .filter_map(|(at, moves, min, max)| {
            let edge = if moves > Num::ZERO { max } else { min };
            (edge - at).checked_div(moves, Rounding::Floor)
        })
        .min()
        .expect("a unit vector moves along an axis")
        .max(Num::ZERO)
    }

    /// The point of the bounds nearest `[x, z]`.
    pub fn clamp_ground(self, [x, z]: [Num; 2]) -> [Num; 2] {
        [
            x.clamp(self.min[0], self.max[0]),
            z.clamp(self.min[1], self.max[1]),
        ]
    }

    /// The point of the bounds nearest `position` on the ground plane, at its height.
    pub fn clamp(self, position: Vec3) -> Vec3 {
        self.ground_point([position.x, position.z], position)
    }

    /// The point of the bounds nearest `[x, z]` on the ground plane, at the height of `position`: a
    /// unit ordered to a point on the ground keeps its height.
    pub fn ground_point(self, ground: [Num; 2], position: Vec3) -> Vec3 {
        let [x, z] = self.clamp_ground(ground);
        Vec3::new(x, position.y, z)
    }
}

/// A map's `[bounds]`: `min` and `max` as `[x, z]`, refused unless they make bounds.
impl<'de> Deserialize<'de> for Bounds {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            min: [Num; 2],
            max: [Num; 2],
        }
        let Fields { min, max } = Fields::deserialize(deserializer)?;
        Self::new(min, max).ok_or_else(|| {
            D::Error::custom("bounds need min below max, both within the world's bound")
        })
    }
}

#[cfg(test)]
mod tests;
