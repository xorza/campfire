use std::ops::Deref;

use bevy_ecs::change_detection::{DetectChanges, DetectChangesMut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::observer::On;
use bevy_ecs::query::{Allow, QueryData};
use bevy_ecs::system::Query;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Vec3;
use campfire_sim::{
    DueIndex, SimComponent, StateRewritten, TimedKept, TimedProcess, TimedRuns, UnitDues,
    Unpredicted,
};
use campfire_states::{Replication, SentPredicted};
use campfire_units::{ArrivedShort, Body, Facing, UnitEntity};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::party::Party;
use crate::pathing_grid::PathingGrid;
use crate::progress::Progress;
use crate::segment::Segment;
use crate::walker::Walker;

/// A walker's long route to its destination: the goal it serves, with its ask while it waits for
/// the planner, and the waypoints of the route planned last. A unit with no destination has no
/// goal, and so no ask. Where the walker is along it is its `RouteCursor`, which changes as each
/// waypoint is passed and the route does not. It changes through `RouteMut` alone, which sets its
/// unit's `ArrivedShort` with each change; a write from outside the sim sets it as it ends, by
/// `StateRewritten`. Its hook pushes its unit into the index of the asks at each insert, a
/// spawn's and a restore's alike.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[component(on_insert = DueIndex::<Self>::pushed)]
#[require(ArrivedShort, RouteCursor)]
pub struct Route {
    goal: Option<Goal>,
    waypoints: Vec<Vec3>,
    /// Whether the last waypoint is the goal, not the nearest place to it the walker reaches.
    reached: bool,
}

/// Where a walker is along its route: the next waypoint, first among those left, and where the
/// leg to it began, the walker's place as the route or its last detour began, or the waypoint it
/// passed last; none before its first route.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteCursor {
    next: u32,
    from: Option<Vec3>,
}

/// A route with its cursor, to read what is left of it.
#[derive(Debug, Clone, Copy)]
pub struct RouteRef<'a> {
    route: &'a Route,
    cursor: &'a RouteCursor,
}

/// Where a route goes, and its ask while it waits for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Goal {
    position: Vec3,
    asked: Option<Ask>,
}

/// An ask for a route: the tick it was first asked in, and the party whose route search it
/// shares, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Ask {
    tick: Tick,
    party: Option<Party>,
}

/// A walker's ask for a route: to `goal`, in `tick`, as one of `party` when it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteAsk {
    pub goal: Vec3,
    pub tick: Tick,
    pub party: Option<Party>,
}

impl Route {
    pub fn goal(&self) -> Option<Vec3> {
        self.goal.map(|goal| goal.position)
    }

    pub fn asked(&self) -> Option<Tick> {
        self.goal.and_then(|goal| goal.asked).map(|ask| ask.tick)
    }

    /// The party whose route search the waiting ask shares.
    pub(crate) fn party(&self) -> Option<Party> {
        self.goal?.asked?.party
    }

    pub const fn is_reached(&self) -> bool {
        self.reached
    }

    /// Sets each walker's `ArrivedShort` as its route stands, as the state was written from
    /// outside the sim, a client's rollback among them.
    pub(crate) fn rewritten(
        _: On<'_, '_, StateRewritten>,
        mut walkers: Query<'_, '_, (&Self, &RouteCursor, &mut ArrivedShort), Allow<Unpredicted>>,
    ) {
        for (route, cursor, mut short) in &mut walkers {
            short.set_if_neq(ArrivedShort::of(RouteRef { route, cursor }.short_of()));
        }
    }

    /// Asks the planner for the route `ask` names, and forgets the walker's `progress` along the
    /// route it had; the walker keeps to that route until the planner answers. An ask while one
    /// waits changes the goal and the party and keeps the first tick, so a walker whose goal moves
    /// each tick keeps its place among the routes that wait.
    fn ask(&mut self, ask: RouteAsk, progress: &mut Progress) {
        let RouteAsk { goal, tick, party } = ask;
        let tick = self.asked().unwrap_or(tick);
        self.goal = Some(Goal {
            position: goal,
            asked: Some(Ask { tick, party }),
        });
        progress.restart();
    }

    /// Asks in `tick` for a route to its goal again, alone, as its walker is stuck, and counts the
    /// ask in `progress`.
    const fn ask_stuck(&mut self, progress: &mut Progress, tick: Tick) {
        let goal = self.goal.as_mut().expect("a stuck walker walks to a goal");
        goal.asked = Some(Ask { tick, party: None });
        progress.restart_stuck();
    }

    /// Asks in `tick` for a route to `goal` again, if there is one, as `ask` does: a unit a forced
    /// move left elsewhere walks to it from there.
    fn ask_again(&mut self, goal: Option<Vec3>, progress: &mut Progress, tick: Tick) {
        if let Some(goal) = goal {
            let ask = RouteAsk {
                goal,
                tick,
                party: None,
            };
            self.ask(ask, progress);
        }
    }

    /// Takes the planner's answer to its ask: `waypoints`, the last the goal when `reached`, from
    /// the walker's place `from`.
    fn answer(&mut self, cursor: &mut RouteCursor, from: Vec3, waypoints: &[Vec3], reached: bool) {
        let goal = self
            .goal
            .as_mut()
            .expect("an answer to an ask, which has a goal");
        goal.asked = None;
        self.waypoints.clear();
        self.waypoints.extend_from_slice(waypoints);
        self.reached = reached;
        *cursor = RouteCursor {
            next: 0,
            from: Some(from),
        };
    }

    /// Serves `goal` in place of the goal the route reaches, as its last waypoint.
    fn move_goal(&mut self, goal: Vec3) {
        debug_assert!(self.reached && self.asked().is_none());
        let last = self
            .waypoints
            .last_mut()
            .expect("a route that reaches its goal");
        *last = goal;
        self.goal = Some(Goal {
            position: goal,
            asked: None,
        });
    }

    /// Forgets the goal and the route, and keeps the buffer of waypoints.
    fn clear(&mut self, cursor: &mut RouteCursor) {
        self.goal = None;
        self.waypoints.clear();
        self.reached = false;
        *cursor = RouteCursor::default();
    }

    /// Goes by `short` in place of the next `skipped` waypoints, from the walker's place `from`.
    /// When it replaces the last, the route ends on the goal only if `reached`: if `short` ends
    /// on it.
    fn splice(
        &mut self,
        cursor: &mut RouteCursor,
        from: Vec3,
        short: &[Vec3],
        skipped: usize,
        reached: bool,
    ) {
        let ahead = cursor.next as usize;
        if ahead + skipped == self.waypoints.len() {
            self.reached = reached;
        }
        self.waypoints
            .splice(..ahead + skipped, short.iter().copied());
        *cursor = RouteCursor {
            next: 0,
            from: Some(from),
        };
    }

    /// Marks the next waypoint of `cursor` reached, where the next leg begins.
    fn advance(&self, cursor: &mut RouteCursor) {
        let next = self.waypoints[cursor.next as usize];
        *cursor = RouteCursor {
            next: cursor.next + 1,
            from: Some(next),
        };
    }
}

impl<'a> RouteRef<'a> {
    pub const fn new(route: &'a Route, cursor: &'a RouteCursor) -> Self {
        Self { route, cursor }
    }

    /// The waypoints the walker has yet to reach, the next first.
    pub fn ahead(self) -> &'a [Vec3] {
        &self.route.waypoints[self.cursor.next as usize..]
    }

    /// The legs of the route the walker has yet to walk, from where the leg to the next waypoint
    /// began: the route's own, whatever the place the walker stands at on the way.
    pub(crate) fn legs(self) -> impl Iterator<Item = Segment> + 'a {
        let ahead = self.ahead();
        let starts = self.cursor.from.into_iter().chain(ahead.iter().copied());
        starts.zip(ahead).map(|(from, &to)| Segment::new(from, to))
    }

    /// The way the walker faces on its leg to the next waypoint: from where the leg began to the
    /// waypoint, so it holds for the whole leg, whatever each step's rounding
    /// (04-capabilities/navigation.md, A walker faces its leg and turns at its turn rate); past a
    /// leg of no length, the next leg's. None with no waypoint ahead off where it stands.
    pub fn way(self) -> Option<Facing> {
        let mut from = self.cursor.from?;
        for waypoint in self.ahead() {
            let to = *waypoint;
            if let Some(way) = Facing::along([to.x - from.x, to.z - from.z]) {
                return Some(way);
            }
            from = to;
        }
        None
    }

    /// Whether the walker arrived where its route ends short of its goal: no ask waits, no
    /// waypoint is left, and the last was not the goal. It stays so until it asks again.
    pub(crate) fn is_arrived_short(self) -> bool {
        self.route.goal.is_some_and(|goal| goal.asked.is_none())
            && !self.route.reached
            && self.ahead().is_empty()
    }

    /// The goal it arrived short of, while it stands at that end, as its `ArrivedShort` reports.
    fn short_of(self) -> Option<Vec3> {
        self.route.goal().filter(|_| self.is_arrived_short())
    }
}

/// The route reads as itself.
impl Deref for RouteRef<'_> {
    type Target = Route;

    fn deref(&self) -> &Route {
        self.route
    }
}

/// A walker's route, to change, with its report of the goal it arrived short of, which each
/// change sets as the route then stands, so a reader of the report reads the route's last
/// change.
#[derive(QueryData, Debug)]
#[query_data(mutable, derive(Debug))]
pub struct RouteMut {
    route: &'static mut Route,
    cursor: &'static mut RouteCursor,
    short: &'static mut ArrivedShort,
}

/// A walker's route with its cursor, to read.
#[derive(QueryData, Debug)]
#[query_data(derive(Debug))]
pub struct RouteRead {
    route: &'static Route,
    cursor: &'static RouteCursor,
}

/// The route reads as itself.
impl Deref for RouteReadItem<'_, '_> {
    type Target = Route;

    fn deref(&self) -> &Route {
        self.route
    }
}

/// The route reads as itself.
impl Deref for RouteMutReadOnlyItem<'_, '_> {
    type Target = Route;

    fn deref(&self) -> &Route {
        self.route
    }
}

/// The route reads as itself.
impl Deref for RouteMutItem<'_, '_> {
    type Target = Route;

    fn deref(&self) -> &Route {
        &self.route
    }
}

impl RouteReadItem<'_, '_> {
    pub const fn read(&self) -> RouteRef<'_> {
        RouteRef::new(self.route, self.cursor)
    }
}

impl RouteMutReadOnlyItem<'_, '_> {
    pub const fn read(&self) -> RouteRef<'_> {
        RouteRef::new(self.route, self.cursor)
    }
}

impl RouteMutItem<'_, '_> {
    pub fn read(&self) -> RouteRef<'_> {
        RouteRef::new(&self.route, &self.cursor)
    }

    /// Whether the route changed since the reading system's last run.
    pub fn is_changed(&self) -> bool {
        self.route.is_changed()
    }

    /// See `Route::ask`; the unit is due in `dues` while its ask waits.
    pub fn ask(&mut self, ask: RouteAsk, progress: &mut Progress, dues: &mut UnitDues<'_, Route>) {
        self.route.ask(ask, progress);
        self.report();
        dues.push(Route::due(&(), &self.route));
    }

    /// See `Route::ask_again`; the unit is due in `dues` while its ask waits.
    pub(crate) fn ask_again(
        &mut self,
        goal: Option<Vec3>,
        progress: &mut Progress,
        tick: Tick,
        dues: &mut UnitDues<'_, Route>,
    ) {
        self.route.ask_again(goal, progress, tick);
        self.report();
        dues.push(Route::due(&(), &self.route));
    }

    /// Asks its route again as its walker is stuck, as `Route::ask_stuck` does, or, once it asked
    /// as many as it may so, forgets its goal and its route, so the walker stops where it stands
    /// and drops its destination (04-capabilities/navigation.md, A stuck walker plans again, then
    /// stops); the unit is due in `dues` while its ask waits.
    pub(crate) fn ask_stuck(
        &mut self,
        progress: &mut Progress,
        tick: Tick,
        dues: &mut UnitDues<'_, Route>,
    ) {
        if progress.asked_out() {
            self.clear();
            progress.restart();
            return;
        }
        self.route.ask_stuck(progress, tick);
        self.report();
        dues.push(Route::due(&(), &self.route));
    }

    /// See `Route::answer`.
    pub(crate) fn answer(&mut self, from: Vec3, waypoints: &[Vec3], reached: bool) {
        self.route
            .answer(&mut self.cursor, from, waypoints, reached);
        self.report();
    }

    /// See `Route::move_goal`.
    pub(crate) fn move_goal(&mut self, goal: Vec3) {
        self.route.move_goal(goal);
        self.report();
    }

    /// See `Route::clear`.
    pub fn clear(&mut self) {
        self.route.clear(&mut self.cursor);
        self.report();
    }

    /// See `Route::splice`.
    pub(crate) fn splice(&mut self, from: Vec3, short: &[Vec3], skipped: usize, reached: bool) {
        self.route
            .splice(&mut self.cursor, from, short, skipped, reached);
        self.report();
    }

    /// See `Route::advance`.
    pub(crate) fn advance(&mut self) {
        debug_assert!(!self.read().ahead().is_empty());
        self.route.advance(&mut self.cursor);
        self.report();
    }

    fn report(&mut self) {
        let short = self.read().short_of();
        self.short.set_if_neq(ArrivedShort::of(short));
    }
}

/// A walker whose ask waits is due at once, and stays held until the planner answers, whatever
/// the tick its ask names (04-capabilities/00-overview.md, A standing order is due at once).
impl TimedProcess for Route {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static Self;
    type Books = ();

    fn due((): &(), route: &Self) -> Option<Tick> {
        route.asked().map(|_| Tick::ZERO)
    }
}

impl SimComponent for Route {
    const NAME: &'static str = "navigation.route";
    type Host = UnitEntity;

    // A route is planned over the cells of its unit's kind of walker, which must be one the mode
    // has, as every pass that reads a walking unit's cells takes it to be; each of its places
    // lies within the world's bound, as every place does.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let within = |point: Vec3| point.within_bound();
        let goal = |goal: Goal| {
            let party = goal.asked.and_then(|ask| ask.party);
            within(goal.position) && party.is_none_or(|party| within(party.goal))
        };
        let places = self.goal.is_none_or(goal) && self.waypoints.iter().copied().all(within);
        let served = world.get_resource::<PathingGrid>().is_none_or(|grid| {
            Walker::of(world.get::<Body>(entity))
                .is_some_and(|walker| grid.serving(walker).is_some())
        });
        let cursor = world.get::<RouteCursor>(entity);
        served && places && cursor.is_some_and(|cursor| cursor.fits(self))
    }
}

impl Replication for Route {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}

impl RouteCursor {
    /// Whether it is a cursor of `route`: its next waypoint at most one past the route's last, a
    /// leg ahead from a place it began at, which lies within the world's bound, as every place
    /// does. A restore checks it of the route and the cursor alike, so neither leaves the other
    /// out of step.
    fn fits(&self, route: &Route) -> bool {
        let next = self.next as usize;
        let within = next <= route.waypoints.len();
        let started = self.from.is_some() || next == route.waypoints.len();
        let placed = self.from.is_none_or(Vec3::within_bound);
        within && started && placed
    }
}

impl SimComponent for RouteCursor {
    const NAME: &'static str = "navigation.route_cursor";
    type Host = UnitEntity;

    // Its route's, as `fits` says.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<Route>(entity)
            .is_some_and(|route| self.fits(route))
    }
}

impl Replication for RouteCursor {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::bundle::{Bundle, NoBundleEffect};
    use campfire_common::Tick;
    use campfire_math::Vec3;

    use crate::progress::Progress;
    use crate::route::{Route, RouteAsk, RouteCursor, RouteRef};

    /// A route a test holds, with its cursor, to insert on a walker.
    #[derive(Debug, Clone)]
    pub struct PlannedRoute {
        route: Route,
        cursor: RouteCursor,
    }

    impl PlannedRoute {
        /// The route to `goal` that the planner gave from `from` by `waypoints`, the last the
        /// goal: for a test that holds a route no plan of its match would give.
        pub fn new(goal: Vec3, from: Vec3, waypoints: &[Vec3]) -> Self {
            let mut route = Route::default();
            let mut cursor = RouteCursor::default();
            let ask = RouteAsk {
                goal,
                tick: Tick::new(0),
                party: None,
            };
            route.ask(ask, &mut Progress::default());
            let reached = waypoints.last() == Some(&goal);
            route.answer(&mut cursor, from, waypoints, reached);
            Self { route, cursor }
        }

        pub const fn read(&self) -> RouteRef<'_> {
            RouteRef::new(&self.route, &self.cursor)
        }

        /// The route and its cursor, as a walker takes them.
        pub fn parts(self) -> impl Bundle<Effect: NoBundleEffect> {
            (self.route, self.cursor)
        }
    }

    impl Route {
        /// Asks as `RouteMut` does, for a route a test holds outside its unit, which reports to
        /// no `ArrivedShort`.
        pub fn ask_unreported(&mut self, ask: RouteAsk) {
            self.ask(ask, &mut Progress::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Num;

    use super::*;

    #[test]
    fn a_route_faces_each_legs_way_from_where_the_leg_began() {
        fn degrees(route: &Route, cursor: &RouteCursor) -> Option<Num> {
            RouteRef::new(route, cursor).way().map(Facing::degrees)
        }
        fn legs(route: &Route, cursor: &RouteCursor) -> Vec<Segment> {
            RouteRef::new(route, cursor).legs().collect()
        }
        let at = |x: i16, z: i16| Vec3::from((x, 0, z));
        let mut route = Route::default();
        let mut cursor = RouteCursor::default();
        assert_eq!(degrees(&route, &cursor), None);
        assert_eq!(legs(&route, &cursor), []);
        // From (0, 0), to (0, 0) again, then (4, 0), then (4, −3): the leg of no length is passed
        // over, so the way is along `x`, 0°. Its legs are its own three.
        let ask = RouteAsk {
            goal: at(4, -3),
            tick: Tick::new(0),
            party: None,
        };
        route.ask(ask, &mut Progress::default());
        route.answer(
            &mut cursor,
            at(0, 0),
            &[at(0, 0), at(4, 0), at(4, -3)],
            true,
        );
        assert_eq!(degrees(&route, &cursor), Some(Num::ZERO));
        let leg = |from, to| Segment::new(from, to);
        assert_eq!(
            legs(&route, &cursor),
            [
                leg(at(0, 0), at(0, 0)),
                leg(at(0, 0), at(4, 0)),
                leg(at(4, 0), at(4, -3))
            ]
        );
        // Past (0, 0) and (4, 0), the leg from (4, 0) to (4, −3) is along `−z`, 90°, wherever
        // the walker stands on it, and the one leg left.
        route.advance(&mut cursor);
        route.advance(&mut cursor);
        assert_eq!(degrees(&route, &cursor), Some(Num::int(90)));
        assert_eq!(legs(&route, &cursor), [leg(at(4, 0), at(4, -3))]);
        // A detour from (5, −1) to (4, −1), along `−x`, 180°, in place of the next waypoint.
        route.splice(&mut cursor, at(5, -1), &[at(4, -1)], 0, false);
        assert_eq!(degrees(&route, &cursor), Some(Num::int(180)));
        // At the end, none; cleared, none.
        route.advance(&mut cursor);
        route.advance(&mut cursor);
        assert_eq!(degrees(&route, &cursor), None);
        route.clear(&mut cursor);
        assert_eq!(degrees(&route, &cursor), None);
    }

    #[test]
    fn a_cursor_past_its_route_or_ahead_with_no_leg_start_fails_its_check() {
        // A route from (0, 0) to (1, 0), one waypoint: its cursor checks at the start; one ahead
        // with no place its leg began fails; past the last waypoint it needs none; two past it
        // fails, whatever its start.
        let mut route = Route::default();
        let mut cursor = RouteCursor::default();
        let ask = RouteAsk {
            goal: Vec3::from((1, 0, 0)),
            tick: Tick::new(0),
            party: None,
        };
        route.ask(ask, &mut Progress::default());
        route.answer(
            &mut cursor,
            Vec3::from((0, 0, 0)),
            &[Vec3::from((1, 0, 0))],
            true,
        );
        let mut world = World::new();
        let entity = world.spawn(route).id();
        let checks = |cursor: RouteCursor| cursor.check(&world, entity);
        assert!(checks(cursor));
        assert!(!checks(RouteCursor {
            from: None,
            ..cursor
        }));
        assert!(checks(RouteCursor {
            next: 1,
            from: None
        }));
        assert!(!checks(RouteCursor { next: 2, ..cursor }));
    }
}
