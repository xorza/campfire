use campfire_geometry::{Placed, Shape};
use campfire_math::{Num, Vec3};
use campfire_values::{ActionId, Rank};

use crate::action::{Action, Aim};
use crate::action_call::{ActionCall, ResolvedCast};
use crate::action_range::ActionRange;
use crate::action_target::ActionTarget;
use crate::rank_values::RankValues;
use crate::slot_aim::SlotAim;
use crate::targets::Targets;

/// An action that passes its checks: its id, the target it keeps, none for an action that takes
/// none whatever its order named, and the action and its values at the slot's rank.
#[derive(Debug)]
pub struct CheckedAction<'a> {
    pub id: ActionId,
    pub target: ActionTarget,
    pub action: &'a Action,
    pub rank: Rank,
    pub values: RankValues,
}

impl CheckedAction<'_> {
    /// How the cast `call` it checked resolves: at the target it keeps.
    pub const fn resolved(&self, call: ActionCall) -> ResolvedCast {
        let aim = SlotAim {
            slot: call.aim.slot,
            target: self.target,
        };
        ResolvedCast {
            call: ActionCall {
                aim,
                start: call.start,
            },
            values: self.values,
        }
    }

    /// Moves a point it aims at beyond its range in to the range, along the line from the unit
    /// at `from` with a body of `shape`, when its aim clamps, as `targets` measure reach: the
    /// farthest point along the line that it reaches.
    pub fn clamp(&mut self, from: Vec3, shape: Shape, targets: &Targets<'_, '_>) {
        let (Aim::Point { clamp: true }, ActionRange::Meters(range), ActionTarget::Point(at)) =
            (self.action.aim, self.values.range, self.target)
        else {
            return;
        };
        if targets.reaches_point(Placed::new(from, shape), range, at) {
            return;
        }
        let reaches = |step: Num| {
            targets.reaches_point(
                Placed::new(from, shape),
                range,
                targets.toward(from, at, step),
            )
        };
        let most = range
            .checked_add(shape.bound())
            .expect("a reach past every number reaches every point");
        let step = match shape {
            // The step rounds once in each coordinate, so it may end a last bit past the reach;
            // stepping a bit shorter each time ends within it after a few.
            Shape::Circle(_) => {
                let mut step = most;
                while !reaches(step) {
                    step -= Num::from_bits(1);
                }
                step
            }
            // A box's edge lies anywhere within its bound along the line, so the farthest step
            // that reaches is searched for: the position reaches, as it lies inside the box, and
            // a step past the bound and the range does not.
            Shape::Box(_) => {
                let (mut reaching, mut past) = (Num::ZERO, most + Num::from_bits(1));
                while past - reaching > Num::from_bits(1) {
                    let middle = Num::from_bits(i64::midpoint(reaching.to_bits(), past.to_bits()));
                    if reaches(middle) {
                        reaching = middle;
                    } else {
                        past = middle;
                    }
                }
                reaching
            }
        };
        self.target = ActionTarget::Point(targets.toward(from, at, step));
    }

    /// Where a unit walks to come in range of its target: the living unit's place, or the point;
    /// none for an action that aims at nothing or along a direction.
    pub fn aimed_at(&self, targets: &Targets<'_, '_>) -> Option<Vec3> {
        match (self.action.aim, self.target) {
            (Aim::Unit(_), ActionTarget::Unit(target)) => {
                targets.living(target).map(|unit| unit.pos)
            }
            (Aim::Point { .. }, ActionTarget::Point(at)) => Some(at),
            _ => None,
        }
    }

    /// Whether its target is within its range of a unit at `from` with a body of `shape`,
    /// as `targets` measure reach: a unit's body, or a point it aims at; an action of global
    /// reach, one that aims at a direction, or one with no target always is. The range counts
    /// only when an action starts.
    pub fn in_range(&self, from: Vec3, shape: Shape, targets: &Targets<'_, '_>) -> bool {
        let ActionRange::Meters(range) = self.values.range else {
            return true;
        };
        match (self.action.aim, self.target) {
            (Aim::Unit(_), ActionTarget::Unit(target)) => targets
                .living(target)
                .is_some_and(|unit| targets.reaches(Placed::new(from, shape), range, &unit)),
            (Aim::Point { .. }, ActionTarget::Point(at)) => {
                targets.reaches_point(Placed::new(from, shape), range, at)
            }
            _ => true,
        }
    }
}
