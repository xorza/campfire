use campfire_geometry::Bounds;
use campfire_math::{Num, Vec3};

/// The smallest rectangle of the ground plane, its sides along x and z, that holds a group's
/// positions, each corner `[x, z]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupBox {
    low: [Num; 2],
    high: [Num; 2],
}

impl GroupBox {
    /// The box of `positions`; `None` for none.
    pub fn of(positions: impl IntoIterator<Item = Vec3>) -> Option<Self> {
        positions.into_iter().fold(None, |held: Option<Self>, at| {
            let point = [at.x, at.z];
            Some(held.map_or(
                Self {
                    low: point,
                    high: point,
                },
                |held| Self {
                    low: [held.low[0].min(at.x), held.low[1].min(at.z)],
                    high: [held.high[0].max(at.x), held.high[1].max(at.z)],
                },
            ))
        })
    }

    /// The box's midpoint: each coordinate the half of its two ends' sum, rounded toward negative
    /// infinity.
    pub(crate) fn center(self) -> [Num; 2] {
        // Both ends lie within the world's bound, far inside an i64's half.
        let mid = |axis: usize| {
            Num::from_bits((self.low[axis].to_bits() + self.high[axis].to_bits()) >> 1)
        };
        [mid(0), mid(1)]
    }

    /// Whether `point` lies inside the box or on its edge.
    pub fn holds(self, [x, z]: [Num; 2]) -> bool {
        self.low[0] <= x && x <= self.high[0] && self.low[1] <= z && z <= self.high[1]
    }

    /// The goal of the unit at `position` of a group ordered to `goal`, a point within `bounds`:
    /// outside the box, `goal` plus the unit's offset from the center, taken into the bounds;
    /// inside the box or on its edge, `goal` itself.
    pub fn goal_of(self, position: Vec3, goal: [Num; 2], bounds: Bounds) -> [Num; 2] {
        if self.holds(goal) {
            return goal;
        }
        let [cx, cz] = self.center();
        bounds.clamp_ground([goal[0] + (position.x - cx), goal[1] + (position.z - cz)])
    }
}

#[cfg(test)]
mod tests;
