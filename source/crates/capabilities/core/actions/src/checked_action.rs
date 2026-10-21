use campfire_geometry::{Placed, Shape};
use campfire_math::{Num, Vec3};
use campfire_units::Body;
use campfire_values::{ActionId, Rank};

use crate::action::{Action, Aim};
use crate::action_call::{ActionCall, ResolvedCast};
use crate::action_range::ActionRange;
use crate::action_target::ActionTarget;
use crate::range_fit::RangeFit;
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

/// What a checked action aims at: a living unit's body or a point, a unit that lives no more, or
/// nothing, as an action of no target or one along a direction.
#[derive(Debug, Clone, Copy)]
enum Aimed {
    At(Placed),
    Gone,
    Nothing,
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
        let range = range.get();
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

    /// How its target lies from a unit at `from` with `body`, as `targets` measure reach: against
    /// its range and its minimum range from a unit's body, or of a point it aims at; within reach
    /// for an action of global reach and no minimum, one that aims at a direction, or one with no
    /// target; beyond it for a unit that lives no more. A kind tests its own bounds of reach past
    /// it, as a weapon its pitch. The reach counts only when an action starts.
    pub fn reach(&self, from: Vec3, body: Option<&Body>, targets: &Targets<'_, '_>) -> RangeFit {
        let from = Placed::new(from, Body::shape_of(body));
        match self.aimed(targets) {
            Aimed::At(to) => targets.reach(from, self.values.range, self.values.min_range, to),
            Aimed::Gone => RangeFit::Far,
            Aimed::Nothing => RangeFit::Within,
        }
    }

    /// Where a unit at `from` with `body` walks to bring its target, which lies `reach` from it,
    /// within its reach: to the living unit's place or the point beyond its range, and straight
    /// away from it to halfway between its minimum and its range nearer than its minimum; none
    /// within reach, or for an action that aims at nothing or along a direction.
    pub fn walk_to(
        &self,
        from: Vec3,
        body: Option<&Body>,
        reach: RangeFit,
        targets: &Targets<'_, '_>,
    ) -> Option<Vec3> {
        let Aimed::At(to) = self.aimed(targets) else {
            return None;
        };
        match reach {
            RangeFit::Within => None,
            RangeFit::Far => Some(to.position),
            RangeFit::Near => {
                let from = Placed::new(from, Body::shape_of(body));
                Some(targets.away(from, to, self.values.midway()?))
            }
        }
    }

    /// What it aims at, as `targets` hold it.
    fn aimed(&self, targets: &Targets<'_, '_>) -> Aimed {
        match (self.action.aim, self.target) {
            (Aim::Unit(_), ActionTarget::Unit(target)) => match targets.living(target) {
                Some(unit) => Aimed::At(Placed::new(unit.position, unit.shape)),
                None => Aimed::Gone,
            },
            (Aim::Point { .. }, ActionTarget::Point(at)) => Aimed::At(Placed::point(at)),
            _ => Aimed::Nothing,
        }
    }
}
