use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::query::QueryData;
use bevy_ecs::world::World;
use campfire_actions::{ActionProgress, ActionSlots, ActionTarget, SlotTimes};
use campfire_common::Tick;
use campfire_geometry::Bounds;
use campfire_math::{Num, Vec3};
use campfire_navigation::{Party, PathWalker, Progress, Route};
use campfire_production::{
    BuildOrder, BuildTarget, Builder, GatherOrder, GatherStep, Gatherer, NodeAt,
};
use campfire_script::{Effect, Frame};
use campfire_sim::{Position, SimWorld, StableId};
use campfire_units::{Destination, MapBook, SpawnPoint};
use campfire_values::ActionId;

use crate::resetting::Resetting;

/// An order to one unit, as every source gives it once it checked it: a player's command, a
/// bot's input, or an order an AI call queued for the unit that thinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitOrder {
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
    /// Use the object `target` with the use slot `slot`, filled with `action`, the object's, unless
    /// it holds it already, as `Slot` starts an action.
    Use {
        slot: u8,
        action: ActionId,
        target: StableId,
    },
}

/// The parts of a unit that an order reads and changes: where it stands and where it spawned, its
/// actions and their times, the path it walks, where it walks to, its route there, and its build
/// and gather orders.
#[derive(Debug, QueryData)]
#[query_data(mutable, derive(Debug))]
pub struct OrderedUnit {
    pub pos: &'static Position,
    pub(crate) spawn: Option<&'static SpawnPoint>,
    pub(crate) slots: Option<&'static mut ActionSlots>,
    pub(crate) times: Option<&'static mut SlotTimes>,
    pub(crate) underway: Option<&'static mut ActionProgress>,
    pub(crate) walker: Option<&'static mut PathWalker>,
    pub(crate) destination: Option<&'static mut Destination>,
    pub(crate) route: Option<&'static mut Route>,
    pub(crate) progress: Option<&'static mut Progress>,
    pub(crate) builder: Option<&'static mut Builder>,
    pub(crate) gatherer: Option<&'static mut Gatherer>,
}

/// For the unit that thinks in the call, which checked the order against the units as the phase
/// began; no unit dies within Think. A unit that resets takes none, and a reset marks the unit
/// as resetting until it is home.
impl Effect for UnitOrder {
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let now = frame.now();
        let id = frame
            .acting()
            .expect("an order comes from the unit that thinks");
        let entity = world.entity_of(id).expect("a unit that thinks lives");
        let bounds = world.resource::<MapBook>().bounds;
        let mut unit = world.entity_mut(entity);
        if unit.contains::<Resetting>() {
            return;
        }
        let parts = unit
            .get_components_mut::<OrderedUnit>()
            .expect("an ordered unit stands");
        self.apply(parts, &bounds, now);
        if self == Self::Reset {
            unit.insert(Resetting);
        }
    }
}

impl UnitOrder {
    /// Applies the order, which its source checked, to `unit`, within `bounds`, in `now`: every
    /// order of every source applies here. A move or a reset drops the unit's target and leaves its
    /// path until it is told to follow it again; a move's point clamps to the bounds, and a slot's
    /// point to the ground within them, at the unit's height. A group's move asks for its route at
    /// once, as one of its party. An attack on another target cancels one in its windup; a slot's
    /// cast or train replaces an action not resolved yet, or releases the charge of its own slot; a
    /// use fills its slot afresh with its object's action, unless it holds it, and then starts as a
    /// slot's cast. A stop ends what is under way, with nothing spent, drops the target and the
    /// destination, and leaves the path; a queue of trains stays. A build becomes the unit's build
    /// order, its point taken into the bounds, and a gather its gather loop, each dropping the
    /// target and the destination; every other order ends both. Every order cuts a channel and ends
    /// a cast that walks in range, with its walk, and an attack cancels a charge.
    pub fn apply(self, unit: OrderedUnitItem<'_, '_>, bounds: &Bounds, now: Tick) {
        let OrderedUnitItem {
            pos: &at,
            spawn,
            mut slots,
            times,
            mut underway,
            walker,
            mut destination,
            route,
            progress,
            mut builder,
            mut gatherer,
        } = unit;
        if let Some(underway) = &mut underway {
            underway.cut_channel();
            if underway.approaching() {
                underway.stop();
                if let Some(destination) = &mut destination {
                    destination.set_if_neq(Destination::to(None));
                }
            }
        }
        self.set_loops(builder.as_mut(), gatherer.as_mut(), bounds, at.get());
        let ground = |x, z| bounds.ground_point([x, z], at.get());
        let to = match self {
            Self::Attack { target } => {
                if let Some(underway) = &mut underway {
                    underway.cancel_charge();
                    underway.set_attack_target(Some(target));
                }
                return;
            }
            Self::Slot { slot, target } => {
                let target = match target {
                    ActionTarget::Point(point) => ActionTarget::Point(ground(point.x, point.z)),
                    target => target,
                };
                if let Some(underway) = &mut underway {
                    underway.order(slot, target);
                }
                return;
            }
            Self::Use {
                slot,
                action,
                target,
            } => {
                let slots = slots.as_mut().expect("a user has slots");
                let underway = underway
                    .as_mut()
                    .expect("a user's slots have their progress");
                if slots.slot(slot).and_then(|held| held.action) != Some(action) {
                    let mut times = times.expect("a user's slots have times");
                    slots.fill((&mut times, underway), slot, Some(action));
                }
                underway.order(slot, ActionTarget::Unit(target));
                return;
            }
            Self::FollowPath => {
                if let Some(underway) = &mut underway {
                    underway.set_attack_target(None);
                }
                if let Some(mut walker) = walker {
                    walker.rejoin();
                }
                return;
            }
            Self::Move { x, z, .. } => Some(ground(x, z)),
            Self::Reset => Some(
                spawn
                    .expect("a unit the mode spawned holds its spawn place")
                    .get(),
            ),
            Self::Stop => {
                if let Some(underway) = &mut underway {
                    underway.stop();
                }
                None
            }
            Self::Build { .. } | Self::Gather { .. } => None,
        };
        if let Some(underway) = &mut underway {
            underway.set_attack_target(None);
        }
        if let Some(mut walker) = walker {
            walker.leave();
        }
        if let Some(mut destination) = destination {
            destination.set_if_neq(Destination::to(to));
        }
        if let (Self::Move { party, .. }, Some(to), Some(mut route), Some(mut progress)) =
            (self, to, route, progress)
            && party.is_some()
        {
            route.ask(to, now, party);
            progress.restart();
        }
    }

    /// Sets the unit's build order and gather loop: a build's, its point taken into the bounds,
    /// or a gather's, aimed at the node as the unit stands at `pos`; any other order ends both.
    /// A unit with neither keeps its parts untouched.
    fn set_loops(
        self,
        builder: Option<&mut Mut<'_, Builder>>,
        gatherer: Option<&mut Mut<'_, Gatherer>>,
        bounds: &Bounds,
        pos: Vec3,
    ) {
        let build = match self {
            Self::Build { slot, target } => {
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
            Self::Gather { slot, target } => Some(GatherOrder {
                slot,
                node: NodeAt { node: target, pos },
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
