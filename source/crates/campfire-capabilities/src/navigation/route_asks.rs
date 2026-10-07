use std::ops::Range;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::system::Query;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::{Position, StableId};

use crate::navigation::body_index::BodyIndex;
use crate::navigation::group_box::GroupBox;
use crate::navigation::party::{Party, PartyKey};
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::route::Route;
use crate::navigation::route_planner::{RoutePlanner, Walkable};
use crate::navigation::segment::Segment;
use crate::navigation::walker::Walker;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::layer::Layer;

/// The parts of a unit the planning of its route reads and answers.
pub(crate) type Asking = (
    Entity,
    &'static StableId,
    &'static Position,
    &'static mut Route,
    Option<&'static Body>,
);

/// The units whose routes the Move stage plans.
pub(crate) type AskingUnits<'w, 's> = Query<'w, 's, Asking, Without<Dead>>;

/// The asks of one tick's routes and the buffers that plan them, kept between ticks.
#[derive(Debug, Default)]
pub(crate) struct RouteAsks {
    /// The walkers whose routes wait, by the tick they asked in, then by stable id.
    waiting: Vec<Waiting>,
    /// The places in `waiting` of the asks of parties, by party and goal, then stable id.
    parties: Vec<u32>,
    /// The members of the party being planned, by stable id.
    members: Vec<Member>,
    /// The shared routes of the party being planned, one for each layer, one after another.
    shared: Vec<Position>,
    layers: Vec<SharedRoute>,
    /// The route being planned.
    waypoints: Vec<Position>,
}

/// A walker whose route waits for the planner.
#[derive(Debug, Clone, Copy)]
struct Waiting {
    tick: Tick,
    id: StableId,
    entity: Entity,
    party: Option<Party>,
}

/// A member of a party as its plan starts: where it stands, its kind of walker and its own goal.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Member {
    pub(crate) id: StableId,
    pub(crate) entity: Entity,
    pub(crate) at: Position,
    pub(crate) walker: Walker,
    pub(crate) goal: Position,
}

/// The route a party plans once for the members of one layer.
#[derive(Debug, Clone)]
struct SharedRoute {
    layer: Layer,
    waypoints: Range<usize>,
    reached: bool,
}

/// Whether the tick's planning goes on after a party.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Planning {
    Goes,
    Spent,
}

impl RouteAsks {
    /// Plans the asked routes of `units`, by the tick they were asked in, then by stable id, as
    /// the tick's work begins, until `planner` did all the work a tick may: the route that meets
    /// that limit finishes, and the rest wait for the next tick. A party's members plan together
    /// as the turn of the first of them comes. With no pathing grid each route is the straight
    /// line to its goal.
    pub(crate) fn plan(
        &mut self,
        grid: Option<&PathingGrid>,
        statics: &BodyIndex,
        planner: Option<&mut RoutePlanner>,
        units: &mut AskingUnits<'_, '_>,
    ) {
        self.read(units);
        let (Some(grid), Some(planner)) = (grid, planner) else {
            for next in &self.waiting {
                let (.., mut route, _) = units.get_mut(next.entity).expect("a unit read this tick");
                let goal = route.goal().expect("a unit that asked a route has a goal");
                route.answer(&[goal], true);
            }
            return;
        };
        planner.begin_tick();
        for index in 0..self.waiting.len() {
            let next = self.waiting[index];
            let (.., route, _) = units.get(next.entity).expect("a unit read this tick");
            if route.asked().is_none() {
                continue;
            }
            if planner.spent() {
                break;
            }
            let run = next.party.map_or(0..0, |party| self.party_run(party));
            if run.len() < 2 {
                self.plan_alone(grid, statics, planner, units, next.entity);
                continue;
            }
            if self.plan_party(grid, statics, planner, units, run) == Planning::Spent {
                break;
            }
        }
    }

    /// Reads the waiting asks of `units`, and the places of the parties' among them.
    fn read(&mut self, units: &AskingUnits<'_, '_>) {
        self.waiting.clear();
        self.waiting
            .extend(units.iter().filter_map(|(entity, &id, _, route, _)| {
                Some(Waiting {
                    tick: route.asked()?,
                    id,
                    entity,
                    party: route.party(),
                })
            }));
        self.waiting
            .sort_unstable_by_key(|waiting| (waiting.tick, waiting.id));
        self.parties.clear();
        let place = |index: usize| u32::try_from(index).expect("asks fit a u32");
        self.parties.extend(
            (0..self.waiting.len())
                .filter(|&index| self.waiting[index].party.is_some())
                .map(place),
        );
        let waiting = &self.waiting;
        self.parties.sort_unstable_by_key(|&index| {
            let ask = waiting[index as usize];
            (
                RouteAsks::party_key(ask.party.expect("an ask of a party")),
                ask.id,
            )
        });
    }

    /// The order parties sort in: by key, then goal. The ticks their members asked in do not
    /// part them, as an ask that waits keeps its first tick through a later one.
    fn party_key(party: Party) -> (PartyKey, [i64; 3]) {
        let goal = party.goal.get();
        (
            party.key,
            [goal.x.to_bits(), goal.y.to_bits(), goal.z.to_bits()],
        )
    }

    /// The range of `parties` that holds the asks of `party`.
    fn party_run(&self, party: Party) -> Range<usize> {
        let key = RouteAsks::party_key(party);
        let of = |&index: &u32| {
            let ask = self.waiting[index as usize];
            RouteAsks::party_key(ask.party.expect("an ask of a party"))
        };
        let start = self.parties.partition_point(|index| of(index) < key);
        let end = self.parties.partition_point(|index| of(index) <= key);
        start..end
    }

    /// Plans the route of the unit of `entity` by itself.
    fn plan_alone(
        &mut self,
        grid: &PathingGrid,
        statics: &BodyIndex,
        planner: &mut RoutePlanner,
        units: &mut AskingUnits<'_, '_>,
        entity: Entity,
    ) {
        let (_, _, &at, mut route, body) = units.get_mut(entity).expect("a unit read this tick");
        let goal = route.goal().expect("a unit that asked a route has a goal");
        let walkable = RouteAsks::walkable(grid, statics, Walker::walking(body));
        let outcome = planner.plan(walkable, at, goal, &mut self.waypoints);
        route.answer(&self.waypoints, outcome.reached);
    }

    /// Plans the routes of the party whose asks are `run` of `parties`, two at least, by stable
    /// id.
    fn plan_party(
        &mut self,
        grid: &PathingGrid,
        statics: &BodyIndex,
        planner: &mut RoutePlanner,
        units: &mut AskingUnits<'_, '_>,
        run: Range<usize>,
    ) -> Planning {
        let party = self.waiting[self.parties[run.start] as usize]
            .party
            .expect("an ask of a party");
        self.members.clear();
        for &index in &self.parties[run] {
            let entity = self.waiting[index as usize].entity;
            let (_, &id, &at, route, body) = units.get(entity).expect("a unit read this tick");
            self.members.push(Member {
                id,
                entity,
                at,
                walker: Walker::walking(body),
                goal: route.goal().expect("a unit that asked a route has a goal"),
            });
        }
        self.plan_members(
            grid,
            statics,
            planner,
            party,
            &mut |entity, waypoints, reached| {
                let (.., mut route, _) = units.get_mut(entity).expect("a unit read this tick");
                route.answer(waypoints, reached);
            },
        )
    }

    /// Plans the routes of the party's members, which `members` holds: one search for each layer
    /// they move on, which each member then takes from the last waypoint it sees. After each
    /// search its first member's tests run; before each other member's, a spent tick stops, and
    /// the rest wait.
    fn plan_members(
        &mut self,
        grid: &PathingGrid,
        statics: &BodyIndex,
        planner: &mut RoutePlanner,
        party: Party,
        answer: &mut impl FnMut(Entity, &[Position], bool),
    ) -> Planning {
        debug_assert!(self.members.len() >= 2);
        debug_assert!(self.members.is_sorted_by_key(|member| member.id));
        let centre = GroupBox::of(self.members.iter().map(|member| member.at))
            .expect("a party has members")
            .centre();
        self.shared.clear();
        self.layers.clear();
        for index in 0..self.members.len() {
            let member = self.members[index];
            let searched = !self
                .layers
                .iter()
                .any(|shared| shared.layer == member.walker.layer);
            if searched {
                self.search(grid, statics, planner, party, member.walker.layer, centre);
            } else if planner.spent() {
                return Planning::Spent;
            }
            self.follow(grid, statics, planner, member, party, answer);
        }
        Planning::Goes
    }

    /// Plans the shared route of the members on `layer` to the party's goal, for the widest of
    /// their walkers, from the member of that walker nearest `centre`, the lower stable id on a
    /// tie.
    fn search(
        &mut self,
        grid: &PathingGrid,
        statics: &BodyIndex,
        planner: &mut RoutePlanner,
        party: Party,
        layer: Layer,
        centre: [Num; 2],
    ) {
        let on_layer = self
            .members
            .iter()
            .filter(|member| member.walker.layer == layer);
        let widest = on_layer
            .clone()
            .map(|member| member.walker)
            .max()
            .expect("a layer of a member");
        let squared = |value: Num| u128::from(value.to_bits().unsigned_abs()).pow(2);
        let distance = |at: Position| {
            let at = at.get();
            squared(at.x - centre[0]) + squared(at.z - centre[1])
        };
        let leader = on_layer
            .filter(|member| member.walker == widest)
            .min_by_key(|member| (distance(member.at), member.id))
            .expect("the widest walker is a member's");
        let walkable = RouteAsks::walkable(grid, statics, widest);
        let outcome = planner.plan(walkable, leader.at, party.goal, &mut self.waypoints);
        let start = self.shared.len();
        self.shared.extend_from_slice(&self.waypoints);
        self.layers.push(SharedRoute {
            layer,
            waypoints: start..self.shared.len(),
            reached: outcome.reached,
        });
    }

    /// Gives `member` its route from its layer's shared route: from the last waypoint whose
    /// straight line it passes by its own walker's test, the waypoints tested from the goal back,
    /// on to its own goal when the line to it from the waypoint before the party's goal passes,
    /// else to the party's goal. A member that sees no waypoint plans its own route.
    fn follow(
        &mut self,
        grid: &PathingGrid,
        statics: &BodyIndex,
        planner: &mut RoutePlanner,
        member: Member,
        party: Party,
        answer: &mut impl FnMut(Entity, &[Position], bool),
    ) {
        let shared = self
            .layers
            .iter()
            .find(|shared| shared.layer == member.walker.layer)
            .expect("its layer's route was searched")
            .clone();
        let route = &self.shared[shared.waypoints];
        let walkable = RouteAsks::walkable(grid, statics, member.walker);
        let seen = (0..route.len())
            .rev()
            .find(|&at| planner.sees(walkable, Segment::new(member.at, route[at])));
        let Some(first) = seen else {
            let outcome = planner.plan(walkable, member.at, member.goal, &mut self.waypoints);
            answer(member.entity, &self.waypoints, outcome.reached);
            return;
        };
        self.waypoints.clear();
        self.waypoints.extend_from_slice(&route[first..]);
        let mut reached = shared.reached && member.goal == party.goal;
        if shared.reached && member.goal != party.goal {
            let before = match route.len() - first {
                1 => member.at,
                _ => route[route.len() - 2],
            };
            if planner.sees(walkable, Segment::new(before, member.goal)) {
                *self.waypoints.last_mut().expect("a seen waypoint") = member.goal;
                reached = true;
            }
        }
        answer(member.entity, &self.waypoints, reached);
    }

    fn walkable<'a>(grid: &'a PathingGrid, statics: &'a BodyIndex, walker: Walker) -> Walkable<'a> {
        Walkable {
            clearance: grid.clearance(walker),
            statics,
            short: None,
        }
    }
}

#[cfg(feature = "bench")]
pub(crate) mod internals {
    use bevy_ecs::entity::Entity;
    use campfire_sim::Position;

    use crate::navigation::body_index::BodyIndex;
    use crate::navigation::party::Party;
    use crate::navigation::pathing_grid::PathingGrid;
    use crate::navigation::route_asks::{Member, Planning, RouteAsks};
    use crate::navigation::route_planner::RoutePlanner;

    impl RouteAsks {
        /// Plans the routes of `members` of `party`, two at least, by stable id, as a party's
        /// members plan, and gives each to `answer`, with its entity and whether it reaches the
        /// member's goal.
        pub(crate) fn plan_group(
            &mut self,
            grid: &PathingGrid,
            statics: &BodyIndex,
            planner: &mut RoutePlanner,
            party: Party,
            members: impl IntoIterator<Item = Member>,
            answer: &mut impl FnMut(Entity, &[Position], bool),
        ) -> Planning {
            self.members.clear();
            self.members.extend(members);
            self.plan_members(grid, statics, planner, party, answer)
        }
    }
}
