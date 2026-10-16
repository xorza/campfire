use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::{Num, Vec3};
use campfire_sim::{Position, SimComponent, StableId};
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_values::{ActionStart, DashDelivery};
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// A forced move under way, a dash or a knock back, which moves its unit in the Move stage in
/// place of its own step. While one moves it, a unit takes no step and starts no action, and the
/// action it winds up or channels is interrupted, as a stun would. State of `navigation`, kept
/// here, as `Dead` is, for the capabilities below navigation to read.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForcedMove {
    /// A dash of `step` a tick on the ground plane, more than 0, to `to`; the delivery of the
    /// action that started it, when it is one.
    Dash {
        to: DashTo,
        step: Num,
        delivers: Option<DashDelivery>,
    },
    /// A knock back to `to`, which it reaches in `left` ticks more, at least one: each tick it
    /// goes the share of the way left that one of those ticks is.
    KnockBack { to: Vec3, left: u64 },
}

/// Where a dash goes: to a point, or to a unit, whose place of each tick it follows until their
/// bodies touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DashTo {
    Point(Vec3),
    Unit(StableId),
}

/// Where a tick of a forced move aims on the unit's ground plane, and how near it stops: a dash's
/// unit's place, its body and the unit's apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Goal {
    pub point: Vec3,
    pub reach: Num,
}

/// Where a tick of a forced move takes its unit, and whether the move ends there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Advanced {
    pub point: Vec3,
    pub ends: bool,
}

impl ForcedMove {
    /// One tick of it from `point`, towards `goal`; none for a dash whose unit is gone or dead, which
    /// ends where it is. A dash goes its step, and ends once it comes within its goal's reach; a
    /// knock back goes its share of the way to its end, and ends on it in its last tick.
    pub fn advance(&mut self, point: Vec3, goal: Option<Goal>) -> Advanced {
        let Some(goal) = goal else {
            return Advanced { point, ends: true };
        };
        match self {
            Self::Dash { step, .. } => {
                if point.within(goal.point, goal.reach + *step) {
                    let gap = (point.distance(goal.point) - goal.reach).max(Num::ZERO);
                    return Advanced {
                        point: point.step_toward(goal.point, gap),
                        ends: true,
                    };
                }
                Advanced {
                    point: point.step_toward(goal.point, *step),
                    ends: false,
                }
            }
            Self::KnockBack { left, .. } => {
                if *left <= 1 {
                    return Advanced {
                        point: goal.point,
                        ends: true,
                    };
                }
                let ticks =
                    i64::try_from(*left).expect("a knock back's ticks are within the tick limit");
                let share = point
                    .distance(goal.point)
                    .checked_div_int(ticks)
                    .expect("a share of a way within the bound fits");
                *left -= 1;
                Advanced {
                    point: point.step_toward(goal.point, share),
                    ends: false,
                }
            }
        }
    }
}

impl SimComponent for ForcedMove {
    const NAME: &'static str = "units.forced_move";
    type Host = UnitEntity;

    // A dash of no step, or a knock back with no tick left, never ends. A dash's step, and a
    // knock back's end, lie within twice the world's bound, a distance within it from a place
    // within it, so a step past a body's reach and a way to the end fit a number; a knock back's
    // ticks are a time's, within the tick limit; a dash's point, and the start of the action it
    // delivers, lie within the world's bound, as every place does. That its unit walks, and so is no box, its declarations require.
    fn check(&self, _: &World, _: Entity) -> bool {
        let reach = 2 * Position::BOUND.to_bits().unsigned_abs();
        let within = |value: Num| value.to_bits().unsigned_abs() <= reach;
        match *self {
            Self::Dash { to, step, delivers } => {
                let point = match to {
                    DashTo::Point(point) => Position::from(point).within_bound(),
                    DashTo::Unit(_) => true,
                };
                let start = delivers
                    .and_then(|delivers| delivers.start)
                    .is_none_or(ActionStart::within_bound);
                step > Num::ZERO && within(step) && point && start
            }
            Self::KnockBack { to, left } => {
                (1..=Tick::LIMIT.get()).contains(&left)
                    && [to.x, to.y, to.z].into_iter().all(within)
            }
        }
    }
}

impl Replication for ForcedMove {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
