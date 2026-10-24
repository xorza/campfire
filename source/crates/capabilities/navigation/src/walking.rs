use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Allow, Changed, Has, With, Without};
use bevy_ecs::system::{Commands, If, Local, ParamSet, Query, Res, ResMut};
use campfire_actions::ActionProgress;
use campfire_declarations::{ById, RoleForm};
use campfire_deliveries::DashDelivering;
use campfire_math::{Direction, SinCos, Vec3};
use campfire_sim::{
    DueIndex, EntityIndex, Keyed, MatchRate, Ordered, Position, SimTick, StableId, UnitDues,
    Unpredicted,
};
use campfire_units::{
    AtWork, Body, Dead, Destination, Facing, ForcedMove, Leaving, MapBook, MoveStep, TurnStep,
};
use campfire_values::{Block, Motion, Team, UnitTags, UnitType};

use crate::arrived_at::ArrivedAt;
use crate::aside_depth::AsideDepth;
use crate::body_index::{BodyIndex, IndexedBody};
use crate::broadphase::Broadphase;
use crate::collider::contact::{Contact, ContactBodies};
use crate::collider::{Collider, Pushes};
use crate::crowd_book::CrowdBook;
use crate::crowd_costs::{CrowdCosts, Stamp, Standing};
use crate::gait::Gait;
use crate::hemmed::Hemmed;
use crate::pathing_grid::PathingGrid;
use crate::progress::Progress;
use crate::route::{Route, RouteAsk, RouteMut};
use crate::route_planner::{Passage, RoutePlanner, Short, Walkable, Window};
use crate::stalled_walkers::StalledWalkers;
use crate::steering::{Aside, Friends, Steered, Steering, TickBody};
use crate::unit_navigation_data::UnitNavigationData;
use crate::unparted_pairs::UnpartedPairs;
use crate::walker::Walker;

/// The living units that cannot walk, those a client only holds among them: the static bodies.
type StaticUnits = (
    With<Body>,
    Without<MoveStep>,
    Without<Dead>,
    Allow<Unpredicted>,
);

/// The bodies steering reads each tick: the living units that walk or may, a client's held ones
/// among them, each with its team, its destination, its tags, and what it has under way.
type SteerBodies<'w, 's> = Query<
    'w,
    's,
    (
        (
            &'static StableId,
            &'static Position,
            &'static Body,
            Option<&'static Team>,
        ),
        Option<&'static Destination>,
        Option<&'static UnitTags>,
        (
            Option<&'static ActionProgress>,
            Option<&'static AtWork>,
            Option<&'static ArrivedAt>,
            Option<&'static Hemmed>,
        ),
    ),
    (
        With<MoveStep>,
        Without<Dead>,
        Without<ForcedMove>,
        Allow<Unpredicted>,
    ),
>;

/// The living walkers steering steers, and the friends that step aside among them.
type Steerers<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        (&'static StableId, &'static Position, Option<&'static Team>),
        &'static mut Destination,
        &'static MoveStep,
        &'static Body,
        RouteMut,
        (
            &'static mut Progress,
            Option<&'static mut ArrivedAt>,
            Option<&'static Leaving>,
            Option<&'static mut AsideDepth>,
            Option<&'static mut Hemmed>,
        ),
        Option<&'static UnitTags>,
        Has<ForcedMove>,
    ),
    Without<Dead>,
>;

/// The walkers' moves: steered round each other, a step a tick along their routes, parted where
/// their bodies overlap, and kept in the bounds.
#[derive(Debug)]
pub struct Walking;

impl Walking {
    /// Steers each walker round the units in their way, as `Steering::steer` says, in stable-id
    /// order, with the work the planner has left this tick; once it has none, the rest keep their
    /// routes, and steer again in a later tick. A walker is stuck once it has come no half a step
    /// nearer its waypoint than ever for the ticks of `Progress::stuck_after`, and only coming so
    /// near starts its count again, not a detour. A predicting client counts the units it holds
    /// that stand: one that stands has not moved since the server sent it. With no pathing grid
    /// no walker steers, and one that does not walk this tick, waits for its route, or has none
    /// ahead, does not either: none of them takes a place in the order. On a server, each unit
    /// that stands idle on the next stretch of a walker's way, as far as its window reaches, and
    /// that the mode's `CrowdBook` lets it move, steps aside, as `Steering::asides` says, to the
    /// place `RoutePlanner::yield_place` finds clear of the walker's passage, which
    /// `Steering::passage` reads; one that finds none is hemmed in. A walker stuck for
    /// `Progress::ask_after` asks its route again, and stops past `Progress::MOST_STUCK_ASKS`
    /// such asks; one stuck by its route's last waypoint where a unit that stands crowds it, as
    /// `Steering::crowded` says, passes it, and arrives. A walker that touches the crowd that
    /// arrived at its goal arrives, as `Steering::packed` says; one that leaves its producer takes
    /// no short route round its team's units.
    pub(super) fn steer<F: RoleForm>(
        (rate, index, tick): (Res<'_, MatchRate>, Res<'_, EntityIndex>, Res<'_, SimTick>),
        mut asks: ResMut<'_, DueIndex<Route>>,
        If((grid, mut planner)): If<(Res<'_, PathingGrid>, ResMut<'_, RoutePlanner>)>,
        (statics, crowd, mut stalled): (
            Res<'_, BodyIndex>,
            Res<'_, CrowdBook>,
            ResMut<'_, StalledWalkers>,
        ),
        mut units: ParamSet<'_, '_, (SteerBodies<'_, '_>, Steerers<'_, '_>)>,
        (mut steering, mut order, mut asides, mut passages, mut near): (
            Local<'_, Steering>,
            Local<'_, Ordered>,
            Local<'_, Vec<Aside>>,
            Local<'_, Vec<Vec3>>,
            Local<'_, Vec<IndexedBody>>,
        ),
    ) {
        let bodies = units.p0();
        let read = bodies.iter().map(
            |((&id, &at, body, team), destination, tags, (progress, at_work, arrived, hemmed))| {
                TickBody {
                    body: IndexedBody::of(id, at.get(), body),
                    team: team.copied(),
                    walks: Self::walks(destination, tags, Motion::Own),
                    gathers: tags.is_some_and(|tags| tags.passes()),
                    idle: destination.is_some_and(|destination| destination.get().is_none())
                        && !Self::firm(progress, at_work, tags, hemmed),
                    arrived: arrived
                        .expect("a walker carries the goal it arrived at")
                        .get(),
                }
            },
        );
        steering.read(&statics, read);
        let mut walkers = units.p1();
        let stuck_ticks = Progress::stuck_after(*rate);
        let ask_ticks = Progress::ask_after(*rate);
        let ordered = walkers
            .iter()
            .filter(|(_, _, destination, _, _, route, _, tags, forced)| {
                Self::walks(Some(destination), *tags, Motion::of(*forced))
                    && route.asked().is_none()
                    && !route.read().ahead().is_empty()
            })
            .map(|(entity, (&id, ..), ..)| Keyed { id, entity });
        asides.clear();
        passages.clear();
        let mut stalls = 0;
        for &Keyed { entity, .. } in order.sort(ordered) {
            let (_, (&id, &at, team), mut destination, step, body, mut route, progress, tags, _) =
                walkers.get_mut(entity).expect("a walker in the order");
            let (mut progress, arrived, leaving, walk, _) = progress;
            let mut walk = walk.expect("a walker carries how deep its walk is");
            let depth = *walk;
            let goal = route.goal().expect("a walker with a way ahead has a goal");
            let stuck_for = progress.track(at.get(), route.read().ahead()[0], step.get());
            let steered = Steered {
                id,
                position: at.get(),
                team: team.copied(),
                step: step.get(),
                walker: Walker::walking(Some(body)),
                stuck: stuck_for >= stuck_ticks,
                gathering: tags.is_some_and(|tags| tags.passes()),
                goal,
                depth,
            };
            if steering.packed(steered) {
                let mut arrived = arrived.expect("a walker carries the goal it arrived at");
                Destination::route_to(&mut destination, None);
                ArrivedAt::set(&mut arrived, Some(goal));
                AsideDepth::set(&mut walk, AsideDepth::default());
                route.clear();
                progress.restart();
                continue;
            }
            // One stuck by the last waypoint of its route where a unit that stands crowds it is as
            // near as it gets: it passes it, and the move arrives (walkers-get-through.md, A walker
            // stuck by a crowded goal has arrived).
            if let ([last], true) = (route.read().ahead(), steered.stuck)
                && steering.crowded(steered, *last)
            {
                route.advance();
                progress.passed_waypoint();
                continue;
            }
            if !F::PREDICTS {
                let start = passages.len();
                Steering::passage(&grid, steered, route.read().ahead(), &mut passages);
                let number = |at: usize| u32::try_from(at).expect("a tick's passages fit a u32");
                let passage = number(start)..number(passages.len());
                let before = asides.len();
                let stretch = (route.read().ahead()[0], Steering::reach(&grid));
                steering.asides(steered, stretch, crowd.moves, &passage, &mut asides);
                if asides.len() == before {
                    passages.truncate(start);
                }
            }
            // A walker that leaves its producer passes through its team's units, so it steers
            // round none of them (04-capabilities/navigation.md, A new unit leaves its exit).
            let leaves = leaving.is_some_and(|leaving| leaving.get());
            if let Some(detour) = (!leaves)
                .then(|| {
                    steering.steer(&mut planner, &grid, &statics, steered, route.read().ahead())
                })
                .flatten()
            {
                route.splice(at.get(), steering.short(), detour.skipped, detour.reached);
            }
            // A detour frees a walker only as it walks it, so one that stays stuck asks its route
            // again, detour or none (walkers-get-through.md, A walker progresses only when it
            // comes nearer its waypoint).
            if stuck_for >= ask_ticks {
                let dues = &mut UnitDues::new(&mut asks, id, entity);
                route.ask_stuck(&mut progress, tick.start(), dues);
                stalls += 1;
            }
        }
        stalled.set(stalls);
        // Each friend steps aside once a tick, for the first walker by stable id that moves it;
        // a predicting client moves none, as stepping aside is the server's.
        asides.sort_by_key(|aside| aside.id);
        asides.dedup_by_key(|aside| aside.id);
        for aside in &*asides {
            let Some(entity) = index.get(aside.id) else {
                continue;
            };
            let (
                _,
                (&id, position, team),
                mut destination,
                _,
                body,
                mut route,
                (mut progress, .., walk, hemmed),
                ..,
            ) = walkers
                .get_mut(entity)
                .expect("a friend that stands is a walker");
            let passage = Passage {
                points: &passages[aside.passage.start as usize..aside.passage.end as usize],
                radius: aside.radius,
            };
            let friend = IndexedBody::of(id, position.get(), body);
            // One that its stretch put on the way, but that the walker's route ahead passes, stays.
            if passage.clears(friend) {
                continue;
            }
            let clearance = grid.clearance(Walker::walking(Some(body)));
            let cells = clearance.grid();
            let window = Window::around(
                cells,
                cells.nearest_cell(position.get()),
                Steering::YIELD_WINDOW,
            );
            // A friend that steps ahead of the walker keeps the walker's depth, so a column chains
            // as far as it reaches; one that steps aside goes a step deeper. Its search passes
            // through the idle friends it may move only while its walk may clear them, and again
            // past them as blockers when its place to a side takes it too deep to
            // (walkers-get-through.md, A friend yields through the friends that yield).
            let mut place = |friends| {
                steering.standing_near(&grid, friend, friends, &mut near);
                let short = Short {
                    window,
                    blockers: &near,
                };
                let walkable = Walkable {
                    short: Some(short),
                    ..Walkable::of(clearance, &statics)
                };
                planner.yield_place(walkable, friend, aside.to, passage)
            };
            let side = aside.depth.deeper();
            let mut found = if aside.depth.clears() {
                place(Friends::Pass {
                    team: team.copied(),
                    moves: crowd.moves,
                })
            } else {
                place(Friends::Block)
            };
            if aside.depth.clears()
                && !side.clears()
                && found.is_some_and(|to| !passage.leads_to(to))
            {
                found = place(Friends::Block);
            }
            // One with no place clear of the walker's way stays, firm, and the walker's routes
            // go round it (04-capabilities/navigation.md, A friend that cannot yield is firm,
            // and a stuck walker's route walls it).
            let Some(to) = found else {
                let mut hemmed = hemmed.expect("a walker carries whether it is hemmed in");
                Hemmed::set(&mut hemmed, true);
                continue;
            };
            // It asks its route as it takes its destination, as the routing does, so the moves
            // of this tick do not drop the destination of a walker with no route yet.
            Destination::route_to(&mut destination, Some(to));
            let mut walk = walk.expect("a walker carries how deep its walk is");
            let depth = if passage.leads_to(to) {
                aside.depth
            } else {
                side
            };
            AsideDepth::set(&mut walk, depth);
            let ask = RouteAsk {
                goal: to,
                tick: tick.start(),
                party: None,
            };
            route.ask(
                ask,
                &mut progress,
                &mut UnitDues::new(&mut asks, id, entity),
            );
        }
    }

    /// Counts the units that stand this tick in the crowd's costs, of any team or none, before the
    /// routes are planned: each hemmed in as such, each other that is not firm and that the mode's
    /// `CrowdBook` lets a walker of its team move as one its team's walkers may move, and every
    /// other as one they may not (04-capabilities/navigation.md, A route prices the units on it;
    /// 04-capabilities/navigation.md, A friend that cannot yield is firm, and a stuck walker's
    /// route walls it).
    pub(super) fn price_crowds(
        bodies: SteerBodies<'_, '_>,
        crowd: Res<'_, CrowdBook>,
        costs: Option<ResMut<'_, CrowdCosts>>,
    ) {
        let Some(mut costs) = costs else {
            return;
        };
        let standing = bodies.iter().filter_map(
            |((&id, &at, body, team), destination, tags, (progress, at_work, _, hemmed))| {
                if Self::walks(destination, tags, Motion::Own) {
                    return None;
                }
                let team = team.copied();
                let standing = if hemmed.is_some_and(|hemmed| hemmed.get()) {
                    Standing::Hemmed
                } else if !Self::firm(progress, at_work, tags, hemmed)
                    && crowd.moves.may_move(team, team)
                {
                    Standing::Movable
                } else {
                    Standing::Held
                };
                Some(Stamp {
                    body: IndexedBody::of(id, at.get(), body),
                    team,
                    standing,
                })
            },
        );
        costs.update(standing);
    }

    /// Makes each unit that died since the last Move stage forget where it walked to, and ends its
    /// forced move. No order reaches a dead unit, so it walks nowhere until it lives again.
    pub(super) fn forget_dead(
        mut forced: Query<'_, '_, Entity, (Added<Dead>, With<ForcedMove>)>,
        mut units: Query<
            '_,
            '_,
            (&mut Destination, RouteMut, &mut Progress, &mut Hemmed),
            Added<Dead>,
        >,
        mut commands: Commands<'_, '_>,
    ) {
        for entity in &mut forced {
            commands
                .entity(entity)
                .remove::<(ForcedMove, DashDelivering)>();
        }
        for (mut destination, mut route, mut progress, mut hemmed) in &mut units {
            Hemmed::set(&mut hemmed, false);
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
    /// notes its goal as the one it arrived at, and drops its route when the route reached the
    /// goal: one that ends short stays, arrived short, so an order that sends the unit there again
    /// plans nothing. A unit that walks has arrived at no goal. A unit whose route waits for the
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
                (
                    &mut Progress,
                    Option<&mut ArrivedAt>,
                    Option<&mut AsideDepth>,
                ),
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
            (mut progress, arrived, depth),
            (step, turn),
            tags,
            forced,
        ) in &mut units
        {
            if !Self::walks(Some(&destination), tags, Motion::of(forced)) {
                continue;
            }
            let mut arrived = arrived.expect("a walker carries the goal it arrived at");
            ArrivedAt::set(&mut arrived, None);
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
                        ArrivedAt::set(&mut arrived, route.goal());
                        let mut depth = depth.expect("a walker carries how deep its walk is");
                        AsideDepth::set(&mut depth, AsideDepth::default());
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
                progress.passed_waypoint();
            }
            position.set_if_neq(Position::from(at));
        }
    }

    /// Pushes apart the living bodies of one layer within each other's personal space as the stage
    /// starts, as `Collider::push` says, by the mode's `CrowdBook`, then parts those that overlap,
    /// as `Collider::resolve` says; a pair that comes near only after this tick's pushes parts in
    /// the next. A body a forced move moves passes through the others, and parts from them once
    /// the move ends. Only a unit that can walk is pushed, and one walking to a destination yields
    /// to one that stands. The static bodies' contacts come from `statics`, which holds them as the stage
    /// starts. A predicting client also parts its own units from the units it holds as the server
    /// sent them that cannot walk, such as towers, which never move. Every other held unit is where
    /// the server last had it, behind the client's ticks, and may have started or stopped walking
    /// since, so the server alone parts the client's units from it. Every tick reads the bodies
    /// into `colliders`, finds their contacts with `broadphase`, and writes back the bodies a
    /// contact may have pushed, by their places in `pushed`, buffers it keeps: a body that may
    /// not be pushed, or that no contact names, stays where it stood. A walker the contacts leave
    /// in a static body stands at the nearest place it may, as `stand_clear` says.
    pub(super) fn collide(
        mut units: Query<
            '_,
            '_,
            (
                Entity,
                &StableId,
                &Body,
                &mut Position,
                (
                    &UnitType,
                    Option<&Facing>,
                    Option<&Team>,
                    Option<&mut Leaving>,
                ),
                Option<&Destination>,
                Option<&UnitTags>,
                (Option<&ActionProgress>, Option<&AtWork>, Option<&Hemmed>),
                Has<Unpredicted>,
                Has<ForcedMove>,
            ),
            (With<MoveStep>, Without<Dead>, Allow<Unpredicted>),
        >,
        (statics, navigation, crowd, mut unparted): (
            Res<'_, BodyIndex>,
            Res<'_, ById<UnitType, UnitNavigationData>>,
            Res<'_, CrowdBook>,
            ResMut<'_, UnpartedPairs>,
        ),
        standing: Query<'_, '_, Has<ForcedMove>, StaticUnits>,
        routes: (Option<Res<'_, PathingGrid>>, Option<Res<'_, RoutePlanner>>),
        (mut colliders, mut broadphase, mut pushed, mut among): (
            Local<'_, Vec<Collider>>,
            Local<'_, Broadphase>,
            Local<'_, Vec<usize>>,
            Local<'_, Vec<bool>>,
        ),
    ) {
        colliders.clear();
        colliders.extend(
            units
                .iter()
                .filter(|&(.., held, forced)| !(forced || held))
                .map(
                    |(
                        entity,
                        &id,
                        body,
                        position,
                        (&unit_type, facing, team, leaving),
                        destination,
                        tags,
                        (progress, at_work, hemmed),
                        ..,
                    )| {
                        let UnitNavigationData { gait, weight } =
                            UnitNavigationData::of(&navigation, unit_type);
                        Collider {
                            id,
                            entity,
                            point: position.get(),
                            shape: body.shape(),
                            layer: body.layer(),
                            movable: true,
                            walking: Self::walks(destination, tags, Motion::Own),
                            gathering: tags.is_some_and(|tags| tags.passes()),
                            weight,
                            way: facing
                                .filter(|_| gait == Gait::Drive)
                                .and_then(|&facing| Self::way(facing)),
                            team: team.copied(),
                            firm: Self::firm(progress, at_work, tags, hemmed),
                            leaving: leaving.expect("a walker carries whether it leaves").get(),
                            pinned: false,
                        }
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
        let pushes = Pushes {
            moves: crowd.moves,
            statics: &statics,
        };
        Collider::push(&mut colliders, contacts, pushes);
        unparted.set(Collider::resolve(&mut colliders, contacts));
        if let (Some(grid), Some(planner)) = routes {
            Self::stand_clear(&mut colliders, contacts, &statics, (&grid, &planner));
        }
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
        // A walker that leaves its producer and overlaps none of its team has cleared them, and one
        // that stands has left, so collision parts it from them from the next tick
        // (04-capabilities/navigation.md, A new unit leaves its exit).
        Collider::passing_through(&colliders, contacts, &mut among);
        for (collider, _) in colliders
            .iter()
            .zip(&*among)
            .filter(|&(collider, &among)| collider.leaving && !(among && collider.walking))
        {
            let (_, _, _, _, (.., leaving), ..) = units
                .get_mut(collider.entity)
                .expect("a body read this tick");
            Leaving::set(
                &mut leaving.expect("a walker carries whether it leaves"),
                false,
            );
        }
    }

    /// Puts each walker of a static contact that the contacts left overlapping a static body, as
    /// one wedged between two closer than its body is wide, which each contact pushes into the
    /// other, at the nearest place it may stand on `grid`, as a static body that spawns on a walker
    /// moves it, so no walker stays in a static body (04-capabilities/navigation.md, A walker never
    /// stays in a static body).
    fn stand_clear(
        colliders: &mut [Collider],
        contacts: &[Contact],
        statics: &BodyIndex,
        (grid, planner): (&PathingGrid, &RoutePlanner),
    ) {
        for contact in contacts {
            let ContactBodies::Static { walker, .. } = contact.bodies else {
                continue;
            };
            let collider = &mut colliders[walker];
            if !collider.in_static(statics) {
                continue;
            }
            let kind = Walker {
                layer: collider.layer,
                radius: collider.radius(),
            };
            let walkable = Walkable::of(grid.clearance(kind), statics);
            if let Some(place) = planner.stand_at(walkable, collider.point) {
                collider.point = place;
            }
            debug_assert!(
                !collider.in_static(statics),
                "the walker {:?} stays in a static body at {:?}",
                collider.id,
                collider.point,
            );
        }
    }

    /// Whether a unit is firm, with `progress`, `at_work`, `tags` and `hemmed`: it has an action
    /// under way or a target to attack, it works, its tags root it, or it found no place to step
    /// aside to, as no push or step aside moves such a unit, as Zero Hour's `moveAllies` leaves a
    /// busy ally and StarCraft II's hold position stays (04-capabilities/navigation.md, Who may
    /// move whom is the game's; 04-capabilities/navigation.md, A friend that cannot yield is
    /// firm, and a stuck walker's route walls it).
    fn firm(
        progress: Option<&ActionProgress>,
        at_work: Option<&AtWork>,
        tags: Option<&UnitTags>,
        hemmed: Option<&Hemmed>,
    ) -> bool {
        progress.is_some_and(|progress| {
            progress.in_progress().is_some() || progress.attack_target().is_some()
        }) || at_work.is_some_and(|at_work| at_work.get())
            || tags.is_some_and(|tags| tags.blocks(Motion::Own, Block::Move))
            || hemmed.is_some_and(|hemmed| hemmed.get())
    }

    /// The way `facing` points on the ground plane, in bits along x and z.
    fn way(facing: Facing) -> Option<Direction<2>> {
        let SinCos { sin, cos } = SinCos::of_degrees(facing.degrees());
        Direction::of([i128::from(cos.to_bits()), -i128::from(sin.to_bits())])
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

#[cfg(test)]
mod tests {
    use campfire_math::Num;
    use campfire_units::Facing;

    use crate::walking::Walking;

    #[test]
    fn a_facing_points_from_x_towards_minus_z() {
        // A meter along the way a unit faces lies a meter along its way, as `Facing::along`
        // reads the way back: 0° along +x, 90° along −z, 180° along −x.
        let meter = i128::from(Num::ONE.to_bits());
        for (degrees, off) in [(0, [meter, 0]), (90, [0, -meter]), (180, [-meter, 0])] {
            let way = Walking::way(Facing::of(Num::int(degrees))).unwrap();
            assert_eq!(way.along(off), meter, "{degrees}");
        }
    }
}
