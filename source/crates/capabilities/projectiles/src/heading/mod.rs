use campfire_math::{Num, SinCos, Vec3};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// The way a steered projectile flies, a unit vector in space, as `Vec3::normalized` rounds it
/// (04-capabilities/actions.md, One state for a steered flight's motion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Heading(Vec3);

/// The most a unit's length squared lies from 1, in bits of 2⁻⁴⁸: each component of a unit
/// `Vec3::normalized` gives lies within 2⁻²⁵ of the true unit's, so its square within about
/// 3 · 2 · 2⁻²⁵ ≈ 2⁻²² of 1, which 2⁻²¹ holds with room.
const UNIT_SLACK: u128 = 1 << 27;

impl Heading {
    /// The first heading of a flight from `from` toward `to`, points in space, that leaves along
    /// `way`, a unit: the way, raised by twice the climb to `to` over the ground's distance, of
    /// at least 1 m, as Zero Hour raises a missile over its launch bone's `x`
    /// (`projectileFireAtObjectOrPosition`); the way itself where the raise leaves no length.
    pub(crate) fn launched(way: Vec3, from: Vec3, to: Vec3) -> Self {
        let distance = from.ground_offset(to).length();
        let climb = (to.y - from.y).max(Num::ZERO);
        let up = (climb * Num::int(2)) / distance.max(Num::ONE);
        let raised = Vec3::new(way.x, way.y + up, way.z);
        Self(raised.normalized().unwrap_or(way))
    }

    /// The first heading of a flight from `from` toward `to`, points in space, that leaves along
    /// the ground toward `to`, or along the first axis toward a point straight above or below,
    /// raised as `launched` raises it.
    pub(crate) fn toward(from: Vec3, to: Vec3) -> Self {
        let along = from.ground_offset(to).normalized().unwrap_or(Vec3::new(
            Num::ONE,
            Num::ZERO,
            Num::ZERO,
        ));
        Self::launched(along, from, to)
    }

    /// It turned toward `toward`, a way in space, by at most `turn`, all the way with none; it
    /// stays for a way of no length. Past its bound it turns in the plane of the two ways, about
    /// the vertical's perpendicular when they point apart.
    pub(crate) fn turned(self, toward: Vec3, turn: Option<SinCos>) -> Self {
        let Some(desired) = toward.normalized() else {
            return self;
        };
        let Some(SinCos { sin, cos }) = turn else {
            return Self(desired);
        };
        let along = self.0.dot(desired);
        if along >= cos {
            return Self(desired);
        }
        let side = (desired - self.0 * along)
            .normalized()
            .unwrap_or_else(|| self.perpendicular());
        let turned = self.0 * cos + side * sin;
        Self(turned.normalized().expect("a turn of a unit has a length"))
    }

    pub const fn get(self) -> Vec3 {
        self.0
    }

    /// A unit at a right angle to it: along the ground at a quarter turn from it, or along the
    /// first axis for a heading straight up or down.
    fn perpendicular(self) -> Vec3 {
        Vec3::new(-self.0.z, Num::ZERO, self.0.x)
            .normalized()
            .unwrap_or(Vec3::new(Num::ONE, Num::ZERO, Num::ZERO))
    }
}

/// A snapshot is untrusted, so a heading whose length lies past the rounding of a unit fails to
/// decode, and one that holds is kept as it is, so its next step is the step it took.
impl<'de> Deserialize<'de> for Heading {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let way = Vec3::deserialize(deserializer)?;
        let one = 1_u128 << (2 * Num::FRAC_BITS);
        if way.length_squared_bits().abs_diff(one) > UNIT_SLACK {
            return Err(D::Error::custom("a heading is a unit"));
        }
        Ok(Self(way))
    }
}

#[cfg(test)]
mod tests;
