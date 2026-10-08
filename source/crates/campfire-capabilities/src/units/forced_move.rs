use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{Position, SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::units::action_id::ActionId;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::unit_tags::UnitTags;
use crate::values::action_start::ActionStart;
use crate::values::rank::Rank;

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
    Point(Position),
    Unit(StableId),
}

/// A dash that delivers the instant action `action` of `source` at `rank`, started as `start`
/// says, as that action's `on_resolve` started it: its `on_end` runs as the dash ends. With it,
/// the meters the dash went so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashDelivery {
    pub(crate) source: StableId,
    pub(crate) action: ActionId,
    pub(crate) rank: Rank,
    pub(crate) start: Option<ActionStart>,
    pub(crate) dashed: Num,
}

impl DashDelivery {
    /// The delivery of `action` of `source` at `rank`, started as `start` says, before the dash
    /// went anywhere.
    pub(crate) const fn new(
        source: StableId,
        action: ActionId,
        rank: Rank,
        start: Option<ActionStart>,
    ) -> DashDelivery {
        DashDelivery {
            source,
            action,
            rank,
            start,
            dashed: Num::ZERO,
        }
    }

    /// Adds the way of a step from `from` to `to`. A dash goes at most a step a tick, so only a
    /// match far past any session's length reaches the end of a number, where it stays.
    pub(crate) fn went(&mut self, from: Position, to: Position) {
        self.dashed = self
            .dashed
            .checked_add(from.get().distance(to.get()))
            .unwrap_or(Num::MAX);
    }
}

/// Where a tick of a forced move aims on the unit's ground plane, and how near it stops: a dash's
/// unit's place, its body and the unit's apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Goal {
    pub(crate) at: Vec3,
    pub(crate) reach: Num,
}

/// Where a tick of a forced move takes its unit, and whether the move ends there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Advanced {
    pub(crate) at: Vec3,
    pub(crate) ends: bool,
}

impl ForcedMove {
    /// Whether a unit with `tags`, under a forced move when `forced`, is kept from `block`: by its
    /// tags, or by the forced move from a step, a cast or an attack.
    pub(crate) fn blocks(tags: Option<&UnitTags>, forced: bool, block: Block) -> bool {
        UnitTags::properties_of(tags).blocks(block)
            || forced && matches!(block, Block::Move | Block::Cast | Block::Attack)
    }

    /// One tick of it from `at`, towards `goal`; none for a dash whose unit is gone or dead, which
    /// ends where it is. A dash goes its step, and ends once it comes within its goal's reach; a
    /// knock back goes its share of the way to its end, and ends on it in its last tick.
    pub(crate) fn advance(&mut self, at: Vec3, goal: Option<Goal>) -> Advanced {
        let Some(goal) = goal else {
            return Advanced { at, ends: true };
        };
        match self {
            ForcedMove::Dash { step, .. } => {
                if at.within(goal.at, goal.reach + *step) {
                    let gap = (at.distance(goal.at) - goal.reach).max(Num::ZERO);
                    return Advanced {
                        at: at.step_toward(goal.at, gap),
                        ends: true,
                    };
                }
                Advanced {
                    at: at.step_toward(goal.at, *step),
                    ends: false,
                }
            }
            ForcedMove::KnockBack { left, .. } => {
                if *left <= 1 {
                    return Advanced {
                        at: goal.at,
                        ends: true,
                    };
                }
                let ticks = i64::try_from(*left).expect("a knock back's ticks fit i64");
                let share = at
                    .distance(goal.at)
                    .checked_div_int(ticks)
                    .expect("a share of a way within the bound fits");
                *left -= 1;
                Advanced {
                    at: at.step_toward(goal.at, share),
                    ends: false,
                }
            }
        }
    }
}

impl SimComponent for ForcedMove {
    const NAME: &'static str = "units.forced_move";

    // A dash of no step, or a knock back with no tick left, never ends. A knock back's end lies
    // within twice the world's bound, a distance within it from a place within it, so its way
    // fits a number. A box never walks, so a forced move never moves one.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let walker = world
            .get::<Body>(entity)
            .is_none_or(|body| body.radius().is_some());
        walker
            && match *self {
                ForcedMove::Dash { step, .. } => step > Num::ZERO,
                ForcedMove::KnockBack { to, left } => {
                    let reach = 2 * Position::BOUND.to_bits().unsigned_abs();
                    left > 0
                        && [to.x, to.y, to.z]
                            .iter()
                            .all(|axis| axis.to_bits().unsigned_abs() <= reach)
                }
            }
    }
}
