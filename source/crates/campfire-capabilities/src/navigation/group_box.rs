use campfire_math::Num;
use campfire_sim::Position;

use crate::values::bounds::Bounds;

/// The smallest rectangle of the ground plane, its sides along x and z, that holds a group's
/// positions, each corner `[x, z]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroupBox {
    low: [Num; 2],
    high: [Num; 2],
}

impl GroupBox {
    /// The box of `positions`; `None` for none.
    pub(crate) fn of(positions: impl IntoIterator<Item = Position>) -> Option<GroupBox> {
        positions
            .into_iter()
            .fold(None, |held: Option<GroupBox>, at| {
                let at = at.get();
                let point = [at.x, at.z];
                Some(held.map_or(
                    GroupBox {
                        low: point,
                        high: point,
                    },
                    |held| GroupBox {
                        low: [held.low[0].min(at.x), held.low[1].min(at.z)],
                        high: [held.high[0].max(at.x), held.high[1].max(at.z)],
                    },
                ))
            })
    }

    /// The box's midpoint: each coordinate the half of its two ends' sum, rounded toward negative
    /// infinity.
    pub(crate) fn centre(self) -> [Num; 2] {
        // Both ends lie within the world's bound, far inside an i64's half.
        let mid = |axis: usize| {
            Num::from_bits((self.low[axis].to_bits() + self.high[axis].to_bits()) >> 1)
        };
        [mid(0), mid(1)]
    }

    /// Whether `point` lies inside the box or on its edge.
    pub(crate) fn holds(self, [x, z]: [Num; 2]) -> bool {
        self.low[0] <= x && x <= self.high[0] && self.low[1] <= z && z <= self.high[1]
    }

    /// The goal of the unit at `at` of a group ordered to `goal`, a point within `bounds`: outside
    /// the box, `goal` plus the unit's offset from the centre, taken into the bounds; inside the
    /// box or on its edge, `goal` itself.
    pub(crate) fn goal_of(self, at: Position, goal: [Num; 2], bounds: Bounds) -> [Num; 2] {
        if self.holds(goal) {
            return goal;
        }
        let [cx, cz] = self.centre();
        let at = at.get();
        bounds.clamp_ground([goal[0] + (at.x - cx), goal[1] + (at.z - cz)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_math::Vec3;

    fn at(x: Num, z: Num) -> Position {
        Position::new(Vec3::new(x, Num::ZERO, z)).unwrap()
    }

    #[test]
    fn a_group_keeps_its_shape_only_on_a_move_away_from_its_box() {
        // Three units at (1, 2), (4, 6) and (2, 3): the box runs from (1, 2) to (4, 6), its centre
        // (2.5, 4). A goal of (20, 10) lies outside, so each goes to its offset from the centre
        // added to the goal: (−1.5, −2) → (18.5, 8), (1.5, 2) → (21.5, 12), (−0.5, −1) →
        // (19.5, 9). Bounds of ±21 m take the second to x = 21.
        let int = Num::int;
        let half = |value: i64| Num::int(value) / 2;
        let units = [at(int(1), int(2)), at(int(4), int(6)), at(int(2), int(3))];
        let group = GroupBox::of(units).unwrap();
        assert_eq!(group.centre(), [half(5), int(4)]);
        let bounds = Bounds::new([int(-21), int(-21)], [int(21), int(21)]).unwrap();
        let goals = units.map(|unit| group.goal_of(unit, [int(20), int(10)], bounds));
        assert_eq!(
            goals,
            [[half(37), int(8)], [int(21), int(12)], [half(39), int(9)]]
        );
        // A goal inside the box or on its edge is every unit's goal.
        for inside in [[int(3), int(5)], [int(1), int(2)], [int(4), int(4)]] {
            assert!(
                units
                    .iter()
                    .all(|&unit| group.goal_of(unit, inside, bounds) == inside)
            );
        }
        // Just past the edge, the shape holds again.
        let past = [int(4) + Num::from_bits(1), int(4)];
        assert_ne!(group.goal_of(units[0], past, bounds), past);
        assert_eq!(GroupBox::of([]), None);
    }

    #[test]
    fn the_centre_rounds_toward_negative_infinity() {
        // Ends 1 and 2 bits apart: (−1 + 0) / 2 = −½ bit rounds to −1 bit, (0 + 1) / 2 = ½ to 0.
        let bit = Num::from_bits;
        let group = GroupBox::of([at(bit(-1), bit(0)), at(bit(0), bit(1))]).unwrap();
        assert_eq!(group.centre(), [bit(-1), bit(0)]);
    }
}
