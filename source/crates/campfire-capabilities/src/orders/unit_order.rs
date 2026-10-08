use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::change_detection::Mut;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::{EntityIndex, Position, StableId};

use crate::actions::action_slots::ActionSlots;

use crate::actions::action_target::ActionTarget;
use crate::geometry::bounds::Bounds;
use crate::navigation::destination::Destination;
use crate::navigation::party::Party;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::orders::Orders;
use crate::production::build_target::BuildTarget;
use crate::production::builder::{BuildOrder, Builder};
use crate::production::gatherer::{GatherOrder, GatherStep, Gatherer, NodeAt};
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;

/// An order to one unit, as every source gives it once it checked it: a player's command, a
/// bot's input, or an order an AI call queued for the unit that thinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnitOrder {
    /// Drop the target, and walk to the ground point `x`, `z` off any path, as one of `party`
    /// when a group moves.
    Move {
        x: Num,
        z: Num,
        party: Option<Party>,
    },
    /// Attack `target`, a living enemy a weapon of the unit selects.
    Attack { target: StableId },
    /// Start the action in `slot`, a cast or a train, at `target`, in place of an action not
    /// resolved yet.
    Slot { slot: u8, target: ActionTarget },
    /// Drop the target, and walk the path again.
    FollowPath,
    /// Drop the target, walk to the spawn place off any path, and take no order until there.
    Reset,
    /// End what is under way, drop the target and the destination, and stand off any path.
    Stop,
    /// Build with the build in `slot` at `target`, off any path; the build walks the unit.
    Build { slot: u8, target: BuildTarget },
    /// Gather with the gather in `slot` at `target`, a node, or a drop-off for the load it
    /// carries, off any path; the loop walks the unit.
    Gather { slot: u8, target: StableId },
}

/// The parts of a unit that an order reads and changes: where it stands and where it spawned,
/// its actions, the path it walks, where it walks to, its route there, and its build order.
#[derive(Debug)]
pub(crate) struct OrderedUnit<'a> {
    pub(crate) at: Position,
    pub(crate) spawn: Option<Position>,
    pub(crate) slots: Option<Mut<'a, ActionSlots>>,
    pub(crate) walker: Option<Mut<'a, PathWalker>>,
    pub(crate) destination: Option<Mut<'a, Destination>>,
    pub(crate) route: Option<Mut<'a, Route>>,
    pub(crate) progress: Option<Mut<'a, Progress>>,
    pub(crate) builder: Option<Mut<'a, Builder>>,
    pub(crate) gatherer: Option<Mut<'a, Gatherer>>,
}

/// For the unit that thinks in the call, which checked the order against the units as the phase
/// began; no unit dies within Think.
impl Effect for UnitOrder {
    fn apply(self, world: &mut World, frame: &mut Frame, now: Tick) {
        let unit = frame
            .acting()
            .expect("an order comes from the unit that thinks");
        let entity = world
            .resource::<EntityIndex>()
            .get(unit)
            .expect("a unit that thinks lives");
        Orders::apply_order(world, entity, self, now);
    }
}

impl UnitOrder {
    /// Applies the order, which its source checked, to `unit`, within `bounds`, in `now`: every
    /// order of every source applies here. A move or a reset drops the unit's target and leaves
    /// its path until it is told to follow it again; a move's point clamps to the bounds, and a
    /// slot's point to the ground within them, at the unit's height. A group's move asks for its
    /// route at once, as one of its party. An attack on another target cancels one in its windup;
    /// a slot's cast or train replaces an action not resolved yet, or releases the charge of its
    /// own slot. A stop ends what is under way, with nothing spent, drops the target and the
    /// destination, and leaves the path; a queue of trains stays. A build becomes the unit's
    /// build order, its point taken into the bounds, and a gather its gather loop, each dropping
    /// the target and the destination; every other order ends both. Every order cuts a channel
    /// and ends a cast that walks in range, with its walk, and an attack cancels a charge.
    /// Whether the unit now resets, and takes no order until it is home.
    pub(crate) fn apply(self, unit: OrderedUnit<'_>, bounds: &Bounds, now: Tick) -> bool {
        let OrderedUnit {
            at,
            spawn,
            mut slots,
            walker,
            mut destination,
            route,
            progress,
            mut builder,
            mut gatherer,
        } = unit;
        if let Some(slots) = &mut slots {
            slots.cut_channel();
            if slots.approaching() {
                slots.stop();
                if let Some(destination) = &mut destination {
                    destination.set_if_neq(Destination::to(None));
                }
            }
        }
        self.set_loops(builder.as_mut(), gatherer.as_mut(), bounds, at);
        let ground = |x, z| bounds.ground_point([x, z], at);
        let to = match self {
            UnitOrder::Attack { target } => {
                if let Some(slots) = &mut slots {
                    slots.cancel_charge();
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
            UnitOrder::Move { x, z, .. } => Some(ground(x, z)),
            UnitOrder::Reset => Some(spawn.expect("the call checked the spawn place")),
            UnitOrder::Stop => {
                if let Some(slots) = &mut slots {
                    slots.stop();
                }
                None
            }
            UnitOrder::Build { .. } | UnitOrder::Gather { .. } => None,
        };
        if let Some(slots) = &mut slots {
            slots.set_attack_target(None);
        }
        if let Some(mut walker) = walker {
            walker.leave();
        }
        if let Some(mut destination) = destination {
            destination.set_if_neq(Destination::to(to));
        }
        if let (UnitOrder::Move { party, .. }, Some(to), Some(mut route), Some(mut progress)) =
            (self, to, route, progress)
            && party.is_some()
        {
            route.ask(to, now, party);
            progress.restart();
        }
        self == UnitOrder::Reset
    }

    /// Sets the unit's build order and gather loop: a build's, its point taken into the bounds,
    /// or a gather's, aimed at the node as the unit stands at `at`; any other order ends both.
    /// A unit with neither keeps its parts untouched.
    fn set_loops(
        self,
        builder: Option<&mut Mut<'_, Builder>>,
        gatherer: Option<&mut Mut<'_, Gatherer>>,
        bounds: &Bounds,
        at: Position,
    ) {
        let build = match self {
            UnitOrder::Build { slot, target } => {
                let target = match target {
                    BuildTarget::Point { x, z, angle } => {
                        let [x, z] = bounds.clamp_ground([x, z]);
                        BuildTarget::Point { x, z, angle }
                    }
                    site @ BuildTarget::Site(_) => site,
                };
                Some(BuildOrder { slot, target })
            }
            _ => None,
        };
        if let Some(builder) = builder
            && (build.is_some() || builder.order().is_some())
        {
            builder.set(build);
        }
        let gather = match self {
            UnitOrder::Gather { slot, target } => Some(GatherOrder {
                slot,
                node: NodeAt { node: target, at },
                step: GatherStep::Ordered,
            }),
            _ => None,
        };
        if let Some(gatherer) = gatherer
            && (gather.is_some() || gatherer.order().is_some())
        {
            gatherer.set(gather);
        }
    }
}
