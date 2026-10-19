use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::query::QueryData;
use campfire_actions::{ActionProgress, ActionTarget, ProgressDues};
use campfire_common::Tick;
use campfire_geometry::Bounds;
use campfire_math::{Num, Vec3};
use campfire_navigation::{Party, PathWalker, Progress, RouteAsk, RouteDues, RouteMut};
use campfire_sim::{Position, StableId};
use campfire_units::{BuildTarget, Destination, GroundPoint, OrderKind, SpawnPoint};

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
    /// Use the object `target`, which `interaction` starts in the use slot the object's users
    /// give the unit, as `Slot` starts an action.
    Use { target: StableId },
}

/// The parts of a unit that an order reads and changes: where it stands and where it spawned, its
/// actions' progress, the path it walks, where it walks to, and its route there.
#[derive(Debug, QueryData)]
#[query_data(mutable, derive(Debug))]
pub(crate) struct OrderedUnit {
    pub(crate) id: &'static StableId,
    pub(crate) pos: &'static Position,
    pub(crate) spawn: Option<&'static SpawnPoint>,
    pub(crate) underway: Option<&'static mut ActionProgress>,
    pub(crate) walker: Option<&'static mut PathWalker>,
    pub(crate) destination: Option<&'static mut Destination>,
    pub(crate) route: Option<RouteMut>,
    pub(crate) progress: Option<&'static mut Progress>,
}

/// The indexes an order pushes its unit into: of its actions' progress and of its route's ask.
#[derive(Debug)]
pub struct UnitDues<'a> {
    pub progress: ProgressDues<'a>,
    pub routes: RouteDues<'a>,
}

impl UnitOrder {
    /// Gives the order, which its source checked, to `unit`, within `bounds`, in `now`: every order
    /// of every source is given here, and what it asks of the capabilities that keep the unit's
    /// loops is the kind it gives back, which its caller triggers as `UnitOrdered`. A move or a
    /// reset drops the unit's target and leaves its path until it is told to follow it again; a
    /// move's point clamps to the bounds, and a slot's point to the ground within them, at the
    /// unit's height. A group's move asks for its route at once, as one of its party. An attack
    /// on another target cancels one in its windup; a slot's cast or train replaces an action not
    /// resolved yet, or releases the charge of its own slot. A stop ends what is under way, with
    /// nothing spent, drops the target and the destination, and leaves the path; a queue of trains
    /// stays. A build, its point taken into the bounds, and a gather drop the target and the
    /// destination. Every order cuts a channel and ends a cast that walks in range, with its
    /// walk, and an attack cancels a charge.
    /// What it does to the unit's progress and its route makes the unit due in `dues`.
    pub(crate) fn give(
        self,
        unit: OrderedUnitItem<'_, '_>,
        bounds: &Bounds,
        now: Tick,
        dues: &mut UnitDues<'_>,
    ) -> OrderKind {
        let UnitDues {
            progress: dues,
            routes,
        } = dues;
        let OrderedUnitItem {
            id: _,
            pos: &at,
            spawn,
            mut underway,
            walker,
            mut destination,
            route,
            progress,
        } = unit;
        if let Some(underway) = &mut underway {
            underway.cut_channel(dues);
            if underway.approaching() {
                underway.stop(dues);
                if let Some(destination) = &mut destination {
                    destination.set_if_neq(Destination::to(None));
                }
            }
        }
        let kind = self.kind(bounds, at.get());
        let ground = |x, z| bounds.ground_point([x, z], at.get());
        let to = match self {
            Self::Attack { target } => {
                if let Some(underway) = &mut underway {
                    underway.cancel_charge();
                    underway.set_attack_target(Some(target), dues);
                }
                return kind;
            }
            Self::Slot { slot, target } => {
                let target = match target {
                    ActionTarget::Point(point) => ActionTarget::Point(ground(point.x, point.z)),
                    target => target,
                };
                if let Some(underway) = &mut underway {
                    underway.order(slot, target, dues);
                }
                return kind;
            }
            Self::Use { .. } => return kind,
            Self::FollowPath => {
                if let Some(underway) = &mut underway {
                    underway.set_attack_target(None, dues);
                }
                if let Some(mut walker) = walker {
                    walker.rejoin();
                }
                return kind;
            }
            Self::Move { x, z, .. } => Some(ground(x, z)),
            Self::Reset => Some(
                spawn
                    .expect("a unit the mode spawned holds its spawn place")
                    .get(),
            ),
            Self::Stop => {
                if let Some(underway) = &mut underway {
                    underway.stop(dues);
                }
                None
            }
            Self::Build { .. } | Self::Gather { .. } => None,
        };
        if let Some(underway) = &mut underway {
            underway.set_attack_target(None, dues);
        }
        if let Some(mut walker) = walker {
            walker.leave();
        }
        if let Some(mut destination) = destination {
            destination.set_if_neq(Destination::to(to));
        }
        if let (Self::Move { party, .. }, Some(goal), Some(mut route), Some(mut progress)) =
            (self, to, route, progress)
            && party.is_some()
        {
            let ask = RouteAsk {
                goal,
                tick: now,
                party,
            };
            route.ask(ask, &mut progress, routes);
        }
        kind
    }

    /// What the order asks of the capabilities that keep the unit's loops: a build's, its point
    /// taken into `bounds`, a gather's, aimed at its node from `pos`, a use's, or any other's.
    fn kind(self, bounds: &Bounds, pos: Vec3) -> OrderKind {
        match self {
            Self::Build { slot, target } => {
                let target = match target {
                    BuildTarget::Point {
                        at: GroundPoint { x, z },
                        angle,
                    } => {
                        let [x, z] = bounds.clamp_ground([x, z]);
                        BuildTarget::Point {
                            at: GroundPoint { x, z },
                            angle,
                        }
                    }
                    site @ BuildTarget::Site(_) => site,
                };
                OrderKind::Build { slot, target }
            }
            Self::Gather { slot, target } => OrderKind::Gather {
                slot,
                node: target,
                from: pos,
            },
            Self::Use { target } => OrderKind::Use { target },
            Self::Move { .. }
            | Self::Attack { .. }
            | Self::Slot { .. }
            | Self::FollowPath
            | Self::Reset
            | Self::Stop => OrderKind::Other,
        }
    }
}
