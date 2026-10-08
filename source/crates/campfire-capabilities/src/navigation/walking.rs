use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Allow, Has, With, Without};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_sim::{Keyed, Ordered, Position, StableId, TickRate, Unpredicted};

use crate::geometry::bounds::Bounds;
use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::broadphase::Broadphase;
use crate::navigation::collider::Collider;
use crate::navigation::destination::Destination;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::navigation::route_planner::RoutePlanner;
use crate::navigation::steering::{Steered, Steering};
use crate::navigation::walker::Walker;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::body_grid::Placed;
use crate::units::dead::Dead;
use crate::units::forced_move::ForcedMove;
use crate::units::move_step::MoveStep;
use crate::units::unit_tags::UnitTags;

/// The walkers' moves: steered round each other, a step a tick along their routes, parted where
/// their bodies overlap, and kept in the bounds.
#[derive(Debug)]
pub(super) struct Walking;

impl Walking {
    /// Steers each walker round the units in their way, as `Steering::steer` says, in stable-id
    /// order, with the work the planner has left this tick; once it has none, the rest keep their
    /// routes, and steer again in a later tick. A walker is stuck once it has moved less than half
    /// a step a tick for `Steering::STUCK_MS`, and its progress starts again when it takes a short
    /// route. A predicting client counts the units it holds that stand: one that stands has not
    /// moved since the server sent it. With no pathing grid no walker steers, and one that does not
    /// walk this tick, or waits for its route, does not either.
    pub(super) fn steer(
        rate: Res<'_, TickRate>,
        grid: Option<Res<'_, PathingGrid>>,
        statics: Res<'_, BodyIndex>,
        mut planner: Option<ResMut<'_, RoutePlanner>>,
        bodies: Query<
            '_,
            '_,
            (
                &StableId,
                &Position,
                &Body,
                Option<&Destination>,
                Option<&UnitTags>,
            ),
            (
                With<MoveStep>,
                Without<Dead>,
                Without<ForcedMove>,
                Allow<Unpredicted>,
            ),
        >,
        mut walkers: Query<
            '_,
            '_,
            (
                Entity,
                &StableId,
                &Position,
                &Destination,
                &MoveStep,
                &Body,
                &mut Route,
                &mut Progress,
                Option<&UnitTags>,
                Has<ForcedMove>,
            ),
            Without<Dead>,
        >,
        (mut steering, mut order): (Local<'_, Steering>, Local<'_, Ordered>),
    ) {
        let Some(grid) = grid else {
            return;
        };
        let planner = planner
            .as_deref_mut()
            .expect("a pathing grid comes with its planner");
        let still = bodies
            .iter()
            .filter(|(.., destination, tags)| !Self::walks(*destination, *tags, false))
            .map(|(&id, &at, body, ..)| IndexedBody::of(id, at, body));
        let walking = bodies
            .iter()
            .filter(|(.., destination, tags)| Self::walks(*destination, *tags, false))
            .map(|(&id, &at, body, ..)| Placed {
                id,
                key: body.layer(),
                at,
                shape: body.shape(),
            });
        let gatherers = bodies
            .iter()
            .filter(|&(.., tags)| UnitTags::gathers(tags))
            .map(|(&id, ..)| id);
        steering.read(&statics, still, walking, gatherers);
        let stuck_ticks = rate
            .ticks(Steering::STUCK_MS)
            .expect("a fixed time fits")
            .get();
        let ordered = walkers.iter().map(|(entity, &id, ..)| Keyed { id, entity });
        for &Keyed { entity, .. } in order.sort(ordered) {
            let (_, &id, &at, destination, step, body, mut route, mut progress, tags, forced) =
                walkers.get_mut(entity).expect("a walker in the order");
            if !Self::walks(Some(destination), tags, forced)
                || route.asked().is_some()
                || route.ahead().is_empty()
            {
                continue;
            }
            let steered = Steered {
                id,
                at,
                step: step.get(),
                walker: Walker::walking(Some(body)),
                stuck: u64::from(progress.track(at, step.get())) >= stuck_ticks,
                gathering: UnitTags::gathers(tags),
            };
            if let Some(detour) = steering.steer(planner, &grid, &statics, steered, &route) {
                route.splice(steering.short(), detour.skipped, detour.reached);
                progress.reset();
            }
        }
    }

    /// Makes each unit that died since the last Move stage forget where it walked to, and ends its
    /// forced move. No order reaches a dead unit, so it walks nowhere until it lives again.
    pub(super) fn forget_dead(
        mut forced: Query<'_, '_, Entity, (Added<Dead>, With<ForcedMove>)>,
        mut units: Query<'_, '_, (&mut Destination, &mut Route, &mut Progress), Added<Dead>>,
        mut commands: Commands<'_, '_>,
    ) {
        for entity in &mut forced {
            commands.entity(entity).remove::<ForcedMove>();
        }
        for (mut destination, mut route, mut progress) in &mut units {
            if destination.get().is_some() {
                destination.set_if_neq(Destination::to(None));
            }
            if route.goal().is_some() {
                route.clear();
                progress.restart();
            }
        }
    }

    /// Walks each living unit along its route, a step a tick: past each waypoint it reaches, on to
    /// the next with the rest of its step. Past the last it has arrived, and drops its destination,
    /// and its route when the route reached the goal: one that ends short stays, arrived short, so
    /// an order that sends the unit there again plans nothing. A unit whose route waits for the
    /// planner walks the one it has, if any. One its tags or a forced move stop keeps both.
    pub(super) fn move_units(
        mut units: Query<
            '_,
            '_,
            (
                &mut Position,
                &mut Destination,
                &mut Route,
                &mut Progress,
                &MoveStep,
                Option<&UnitTags>,
                Has<ForcedMove>,
            ),
            Without<Dead>,
        >,
    ) {
        for (mut position, mut destination, mut route, mut progress, step, tags, forced) in
            &mut units
        {
            if !Self::walks(Some(&destination), tags, forced) {
                continue;
            }
            let mut at = position.get();
            let mut left = step.get();
            loop {
                let Some(&waypoint) = route.ahead().first() else {
                    if route.asked().is_none() {
                        destination.set_if_neq(Destination::to(None));
                        if route.reached() {
                            route.clear();
                            progress.restart();
                        }
                    }
                    break;
                };
                let target = waypoint.get();
                if !at.within(target, left) {
                    at = at.step_toward(target, left);
                    break;
                }
                left -= at.distance(target);
                at = target;
                route.advance();
            }
            position.set_if_neq(
                Position::new(at).expect("a step ends between two points within the bound"),
            );
        }
    }

    /// Parts the living bodies of one layer that overlap as the stage starts, pair by pair in
    /// stable-id order; a pair that only overlaps after this tick's pushes parts in the next. A
    /// body a forced move moves passes through the others, and parts from them once the move ends.
    /// Only a unit that can walk is pushed, and one walking to a destination yields to one that
    /// stands. The static bodies' contacts come from `statics`, which holds them as the stage
    /// starts. A predicting client also parts its own units from the units it holds as the server
    /// sent them that cannot walk, such as towers, which never move. Every other held unit is where
    /// the server last had it, behind the client's ticks, and may have started or stopped walking
    /// since, so the server alone parts the client's units from it. Every tick reads the bodies
    /// into `colliders`, and finds their contacts with `broadphase`, buffers it keeps.
    pub(super) fn collide(
        mut units: Query<
            '_,
            '_,
            (
                Entity,
                &StableId,
                &Body,
                &mut Position,
                Has<MoveStep>,
                Option<&Destination>,
                Option<&UnitTags>,
                Has<Unpredicted>,
                Has<ForcedMove>,
            ),
            (Without<Dead>, Allow<Unpredicted>),
        >,
        statics: Res<'_, BodyIndex>,
        mut colliders: Local<'_, Vec<Collider>>,
        mut broadphase: Local<'_, Broadphase>,
    ) {
        colliders.clear();
        colliders.extend(
            units
                .iter()
                .filter(|&(.., movable, _, _, held, forced)| !(forced || held && movable))
                .map(
                    |(entity, &id, body, position, movable, destination, tags, ..)| Collider {
                        id,
                        entity,
                        at: position.get(),
                        shape: body.shape(),
                        layer: body.layer(),
                        movable,
                        walking: Self::walks(destination, tags, false),
                        gathering: UnitTags::gathers(tags),
                    },
                ),
        );
        colliders.sort_unstable_by_key(|collider| collider.id);
        debug_assert_eq!(
            colliders
                .iter()
                .filter(|collider| !collider.movable)
                .count(),
            statics.len(),
            "the static index holds the static bodies as the stage starts"
        );
        let contacts = broadphase.contacts(&colliders, &statics);
        Collider::resolve(&mut colliders, contacts);
        for collider in &*colliders {
            let (_, _, _, mut position, ..) = units
                .get_mut(collider.entity)
                .expect("a body read this tick");
            let parted = Position::new(collider.at).expect("a push stays near the bounds");
            position.set_if_neq(parted);
        }
    }

    /// Whether a unit walks this tick: it has a destination, and neither its tags nor a forced
    /// move, when `forced`, stop it. One they stop stands, to the units round it as to itself.
    fn walks(destination: Option<&Destination>, tags: Option<&UnitTags>, forced: bool) -> bool {
        destination.is_some_and(|destination| destination.get().is_some())
            && !UnitTags::blocks(tags, forced, Block::Move)
    }

    /// Clamps each unit that walks into the bounds; a unit already within them does not change.
    pub(super) fn keep_in_bounds(
        bounds: Res<'_, Bounds>,
        mut units: Query<'_, '_, &mut Position, With<MoveStep>>,
    ) {
        for mut position in &mut units {
            position.set_if_neq(bounds.clamp(*position));
        }
    }
}
