use bevy_ecs::change_detection::Mut;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::{EntityIndex, Position, StableId};

use crate::actions::action_slots::ActionSlots;

use crate::actions::action_target::ActionTarget;
use crate::navigation::destination::Destination;
use crate::navigation::path_walker::PathWalker;
use crate::orders::Orders;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::values::bounds::Bounds;

/// An order to one unit, as every source gives it once it checked it: a player's command, a
/// bot's input, or an order an AI call queued for the unit that thinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnitOrder {
    /// Drop the target, and walk to the ground point `x`, `z` off any path.
    Move { x: Num, z: Num },
    /// Attack `target`, a living enemy a weapon of the unit selects.
    Attack { target: StableId },
    /// Start the action in `slot`, a cast or a train, at `target`, in place of an action not
    /// resolved yet.
    Slot { slot: u8, target: ActionTarget },
    /// Drop the target, and walk the path again.
    FollowPath,
    /// Drop the target, walk to the spawn place off any path, and take no order until there.
    Reset,
}

/// The parts of a unit that an order reads and changes: where it stands and where it spawned,
/// its actions, the path it walks and where it walks to.
#[derive(Debug)]
pub(crate) struct OrderedUnit<'a> {
    pub(crate) at: Position,
    pub(crate) spawn: Option<Position>,
    pub(crate) slots: Option<Mut<'a, ActionSlots>>,
    pub(crate) walker: Option<Mut<'a, PathWalker>>,
    pub(crate) destination: Option<Mut<'a, Destination>>,
}

/// For the unit that thinks in the call, which checked the order against the units as the phase
/// began; no unit dies within Think.
impl Effect for UnitOrder {
    fn apply(self, world: &mut World, frame: &mut Frame, _: Tick) {
        let unit = frame
            .acting()
            .expect("an order comes from the unit that thinks");
        let entity = world
            .resource::<EntityIndex>()
            .get(unit)
            .expect("a unit that thinks lives");
        Orders::apply_order(world, entity, self);
    }
}

impl UnitOrder {
    /// Applies the order, which its source checked, to `unit`, within `bounds`: every order of
    /// every source applies here. A move or a reset drops the unit's target and leaves its path
    /// until it is told to follow it again; a move's point clamps to the bounds, and a slot's
    /// point to the ground within them, at the unit's height. An attack on another target cancels
    /// one in its windup; a slot's cast or train replaces an action not resolved yet. Every order
    /// cuts a channel. Whether the unit now resets, and takes no order until it is home.
    pub(crate) fn apply(self, unit: OrderedUnit<'_>, bounds: &Bounds) -> bool {
        let OrderedUnit {
            at,
            spawn,
            mut slots,
            walker,
            destination,
        } = unit;
        if let Some(slots) = &mut slots {
            slots.cut_channel();
        }
        let ground = |x, z| bounds.ground_point([x, z], at);
        let to = match self {
            UnitOrder::Attack { target } => {
                if let Some(slots) = &mut slots {
                    slots.set_attack_target(Some(target));
                }
                return false;
            }
            UnitOrder::Slot { slot, target } => {
                let target = match target {
                    ActionTarget::Point(point) => {
                        let point = point.get();
                        ActionTarget::Point(ground(point.x, point.z))
                    }
                    target => target,
                };
                if let Some(slots) = &mut slots {
                    slots.order(slot, target);
                }
                return false;
            }
            UnitOrder::FollowPath => {
                if let Some(slots) = &mut slots {
                    slots.set_attack_target(None);
                }
                if let Some(mut walker) = walker {
                    walker.rejoin();
                }
                return false;
            }
            UnitOrder::Move { x, z } => ground(x, z),
            UnitOrder::Reset => spawn.expect("the call checked the spawn place"),
        };
        if let Some(slots) = &mut slots {
            slots.set_attack_target(None);
        }
        if let Some(mut walker) = walker {
            walker.leave();
        }
        if let Some(mut destination) = destination {
            destination.set(Some(to));
        }
        self == UnitOrder::Reset
    }
}
