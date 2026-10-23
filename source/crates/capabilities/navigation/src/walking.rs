use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Allow, Changed, Has, With, Without};
use bevy_ecs::system::{Commands, If, Local, Query, Res, ResMut};
use campfire_deliveries::DashDelivering;
use campfire_sim::{Keyed, MatchRate, Ordered, Position, StableId, Unpredicted};
use campfire_units::{Body, Dead, Destination, Facing, ForcedMove, MapBook, MoveStep, TurnStep};
use campfire_values::{Block, Motion, UnitTags};

use crate::body_index::{BodyIndex, IndexedBody};
use crate::broadphase::Broadphase;
use crate::collider::Collider;
use crate::collider::contact::Contact;
use crate::pathing_grid::PathingGrid;
use crate::progress::Progress;
use crate::route::RouteMut;
use crate::route_planner::RoutePlanner;
use crate::steering::{Steered, Steering, TickBody};
use crate::walker::Walker;

/// The living units that cannot walk, those a client only holds among them: the static bodies.
type StaticUnits = (
    With<Body>,
    Without<MoveStep>,
    Without<Dead>,
    Allow<Unpredicted>,
);

/// The walkers' moves: steered round each other, a step a tick along their routes, parted where
/// their bodies overlap, and kept in the bounds.
#[derive(Debug)]
pub struct Walking;

impl Walking {
    /// Steers each walker round the units in their way, as `Steering::steer` says, in stable-id
    /// order, with the work the planner has left this tick; once it has none, the rest keep their
    /// routes, and steer again in a later tick. A walker is stuck once it has moved less than half
    /// a step a tick for `Steering::STUCK_MS`, and its progress starts again when it takes a short
    /// route. A predicting client counts the units it holds that stand: one that stands has not
    /// moved since the server sent it. With no pathing grid no walker steers, and one that does not
    /// walk this tick, waits for its route, or has none ahead, does not either: none of them
    /// takes a place in the order.
    pub(super) fn steer(
        rate: Res<'_, MatchRate>,
        If((grid, mut planner)): If<(Res<'_, PathingGrid>, ResMut<'_, RoutePlanner>)>,
        statics: Res<'_, BodyIndex>,
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
                RouteMut,
                &mut Progress,
                Option<&UnitTags>,
                Has<ForcedMove>,
            ),
            Without<Dead>,
        >,
        (mut steering, mut order): (Local<'_, Steering>, Local<'_, Ordered>),
    ) {
        let read = bodies
            .iter()
            .map(|(&id, &at, body, destination, tags)| TickBody {
                body: IndexedBody::of(id, at.get(), body),
                walks: Self::walks(destination, tags, Motion::Own),
                gathers: tags.is_some_and(|tags| tags.passes()),
            });
        steering.read(&statics, read);
        let stuck_ticks = rate
            .ticks(Steering::STUCK_MS)
            .expect("a fixed time fits")
            .get();
        let ordered = walkers
            .iter()
            .filter(
                |&(_, _, _, destination, _, _, ref route, _, tags, forced)| {
                    Self::walks(Some(destination), tags, Motion::of(forced))
                        && route.asked().is_none()
                        && !route.read().ahead().is_empty()
                },
            )
            .map(|(entity, &id, ..)| Keyed { id, entity });
        for &Keyed { entity, .. } in order.sort(ordered) {
            let (_, &id, &at, _, step, body, mut route, mut progress, tags, _) =
                walkers.get_mut(entity).expect("a walker in the order");
            let steered = Steered {
                id,
                position: at.get(),
                step: step.get(),
                walker: Walker::walking(Some(body)),
                stuck: u64::from(progress.track(at.get(), step.get())) >= stuck_ticks,
                gathering: tags.is_some_and(|tags| tags.passes()),
            };
            if let Some(detour) =
                steering.steer(&mut planner, &grid, &statics, steered, route.read().ahead())
            {
                route.splice(at.get(), steering.short(), detour.skipped, detour.reached);
                progress.reset();
            }
        }
    }

    /// Makes each unit that died since the last Move stage forget where it walked to, and ends its
    /// forced move. No order reaches a dead unit, so it walks nowhere until it lives again.
    pub(super) fn forget_dead(
        mut forced: Query<'_, '_, Entity, (Added<Dead>, With<ForcedMove>)>,
        mut units: Query<'_, '_, (&mut Destination, RouteMut, &mut Progress), Added<Dead>>,
        mut commands: Commands<'_, '_>,
    ) {
        for entity in &mut forced {
            commands
                .entity(entity)
                .remove::<(ForcedMove, DashDelivering)>();
        }
        for (mut destination, mut route, mut progress) in &mut units {
            if destination.get().is_some() {
                Destination::route_to(&mut destination, None);
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
    ///
    /// First it turns toward its leg's way, at its turn step, or at once with none, and it stands
    /// this tick unless it then faces within `Facing::WITHIN` of the way
    /// (04-capabilities/navigation.md, A walker faces its leg and turns at its turn rate).
    pub(super) fn move_units(
        mut units: Query<
            '_,
            '_,
            (
                (&mut Position, &mut Facing),
                &mut Destination,
                RouteMut,
                &mut Progress,
                (&MoveStep, Option<&TurnStep>),
                Option<&UnitTags>,
                Has<ForcedMove>,
            ),
            Without<Dead>,
        >,
    ) {
        for (
            (mut position, mut facing),
            mut destination,
            mut route,
            mut progress,
            (step, turn),
            tags,
            forced,
        ) in &mut units
        {
            if !Self::walks(Some(&destination), tags, Motion::of(forced)) {
                continue;
            }
            let mut at = position.get();
            if let Some(way) = route.read().way() {
                let turned = facing.toward(way, turn.copied());
                facing.set_if_neq(turned.facing);
                if !turned.faces {
                    continue;
                }
            }
            let mut left = step.get();
            loop {
                let Some(&waypoint) = route.read().ahead().first() else {
                    if route.asked().is_none() {
                        Destination::route_to(&mut destination, None);
                        if route.is_reached() {
                            route.clear();
                            progress.restart();
                        }
                    }
                    break;
                };
                if !at.within(waypoint, left) {
                    at = at.step_toward(waypoint, left);
                    break;
                }
                left -= at.distance(waypoint);
                at = waypoint;
                route.advance();
            }
            position.set_if_neq(Position::from(at));
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
    /// into `colliders`, finds their contacts with `broadphase`, and writes back the bodies a
    /// contact may have pushed, by their places in `pushed`, buffers it keeps: a body that may
    /// not be pushed, or that no contact names, stays where it stood.
    pub(super) fn collide(
        mut units: Query<
            '_,
            '_,
            (
                Entity,
                &StableId,
                &Body,
                &mut Position,
                Option<&Destination>,
                Option<&UnitTags>,
                Has<Unpredicted>,
                Has<ForcedMove>,
            ),
            (With<MoveStep>, Without<Dead>, Allow<Unpredicted>),
        >,
        statics: Res<'_, BodyIndex>,
        standing: Query<'_, '_, Has<ForcedMove>, StaticUnits>,
        (mut colliders, mut broadphase, mut pushed): (
            Local<'_, Vec<Collider>>,
            Local<'_, Broadphase>,
            Local<'_, Vec<usize>>,
        ),
    ) {
        colliders.clear();
        colliders.extend(
            units
                .iter()
                .filter(|&(.., held, forced)| !(forced || held))
                .map(
                    |(entity, &id, body, position, destination, tags, ..)| Collider {
                        id,
                        entity,
                        point: position.get(),
                        shape: body.shape(),
                        layer: body.layer(),
                        movable: true,
                        walking: Self::walks(destination, tags, Motion::Own),
                        gathering: tags.is_some_and(|tags| tags.passes()),
                    },
                ),
        );
        colliders.sort_unstable_by_key(|collider| collider.id);
        debug_assert_eq!(
            standing.iter().filter(|&forced| !forced).count(),
            statics.len(),
            "the static index holds the static bodies as the stage starts"
        );
        let contacts = broadphase.contacts(&colliders, &statics);
        Collider::resolve(&mut colliders, contacts);
        pushed.clear();
        pushed.extend(contacts.iter().flat_map(Contact::walkers));
        pushed.sort_unstable();
        pushed.dedup();
        for &index in &*pushed {
            let collider = &colliders[index];
            let (_, _, _, mut position, ..) = units
                .get_mut(collider.entity)
                .expect("a body read this tick");
            let parted = collider.point;
            position.set_if_neq(Position::from(parted));
        }
    }

    /// Whether a unit walks this tick: it has a destination, and neither its tags nor its
    /// `motion`, when forced, stop it. One they stop stands, to the units round it as to itself.
    fn walks(destination: Option<&Destination>, tags: Option<&UnitTags>, motion: Motion) -> bool {
        destination.is_some_and(|destination| destination.get().is_some())
            && !tags
                .copied()
                .unwrap_or_default()
                .blocks(motion, Block::Move)
    }

    /// Clamps each unit that walks into the bounds: those whose place changed since the last run,
    /// as a place comes with each unit, spawned or restored, and the bounds, a book's, never
    /// change; a unit already within them does not change.
    pub(super) fn keep_in_bounds(
        map: Res<'_, MapBook>,
        mut units: Query<'_, '_, &mut Position, (With<MoveStep>, Changed<Position>)>,
    ) {
        for mut position in &mut units {
            position.set_if_neq(Position::from(map.bounds.clamp(position.get())));
        }
    }
}
