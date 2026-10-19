use std::ops::Range;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::system::Query;
use campfire_common::Tick;
use campfire_math::{Num, Vec3};
use campfire_sim::{Position, StableId};
use campfire_units::{Body, Dead, Layer};

use crate::body_index::BodyIndex;
use crate::group_box::GroupBox;
use crate::party::Party;
use crate::pathing_grid::PathingGrid;
use crate::route::RouteMut;
use crate::route_planner::{RoutePlanner, Walkable};
use crate::segment::Segment;
use crate::walker::Walker;

/// The parts of a unit the planning of its route reads and answers.
pub(crate) type Asking = (
    Entity,
    &'static StableId,
    &'static Position,
    RouteMut,
    Option<&'static Body>,
);

/// The units whose routes the Move stage plans.
pub(crate) type AskingUnits<'w, 's> = Query<'w, 's, Asking, Without<Dead>>;

/// The asks of one tick's routes and the buffers that plan them, kept between ticks.
#[derive(Debug, Default)]
pub(crate) struct RouteAsks {
    /// The asks that wait, by the tick they were asked in, then by stable id, and whether the
    /// tick's plan answered each.
    waiting: Vec<Waiting>,
    /// The places in `waiting` of the asks of parties, by party and goal, then stable id.
    parties: Vec<u32>,
    /// The members of the party being planned, by stable id.
    members: Vec<Member>,
    /// The shared routes of the party being planned, one for each layer, one after another.
    shared: Vec<Vec3>,
    layers: Vec<SharedRoute>,
    /// The route being planned.
    waypoints: Vec<Vec3>,
}

/// A walker's ask for a route, as its unit stood when the plan read it: the tick it was first
/// asked in, the party whose route search it shares, if any, and the walker.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ask {
    pub(crate) tick: Tick,
    pub(crate) party: Option<Party>,
    pub(crate) member: Member,
}

/// An ask that waits, and whether the tick's plan answered it.
#[derive(Debug, Clone, Copy)]
struct Waiting {
    ask: Ask,
    answered: bool,
}

/// A walker a plan answers: its unit, where it stands, its kind of walker and its own goal.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Member {
    pub id: StableId,
    pub(crate) entity: Entity,
    pub position: Vec3,
    pub(crate) walker: Walker,
    pub(crate) goal: Vec3,
}

/// The route a party plans once for the members of one layer.
#[derive(Debug, Clone)]
struct SharedRoute {
    layer: Layer,
    waypoints: Range<usize>,
    reached: bool,
}

/// What a route search reads, and the planner that does it.
#[derive(Debug)]
pub(crate) struct RouteSearch<'a> {
    pub(crate) grid: &'a PathingGrid,
    pub(crate) statics: &'a BodyIndex,
    pub(crate) planner: &'a mut RoutePlanner,
}

/// A route search for the party being planned.
#[derive(Debug)]
struct PartySearch<'s, 'a> {
    search: &'s mut RouteSearch<'a>,
    party: Party,
}

impl<'a> RouteSearch<'a> {
    /// What `walker` may walk.
    fn walkable(&self, walker: Walker) -> Walkable<'a> {
        Walkable::of(self.grid.clearance(walker), self.statics)
    }
}

/// Whether the tick's planning goes on after a party.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Planning {
    Goes,
    Spent,
}

impl RouteAsks {
    /// Plans the routes of the asks `read` took, by the tick they were asked in, then by stable id,
    /// as the tick's work begins, until the planner of `search` did all the work a tick may: the
    /// route that meets that limit finishes, and the rest wait for the next tick. A party's
    /// members plan together as the turn of the first of them comes. With no search each route is
    /// the straight line to its goal. Each route goes to `answer`, with its member and whether it
    /// reaches the member's goal.
    pub(crate) fn plan(
        &mut self,
        search: Option<RouteSearch<'_>>,
        answer: &mut impl FnMut(Member, &[Vec3], bool),
    ) {
        let Some(mut search) = search else {
            for waiting in &self.waiting {
                let member = waiting.ask.member;
                answer(member, &[member.goal], true);
            }
            return;
        };
        search.planner.begin_tick();
        for index in 0..self.waiting.len() {
            let next = self.waiting[index];
            if next.answered {
                continue;
            }
            if search.planner.is_spent() {
                break;
            }
            let run = next.ask.party.map_or(0..0, |party| self.party_run(party));
            if run.len() < 2 {
                let member = next.ask.member;
                let walkable = search.walkable(member.walker);
                let outcome = search.planner.plan(
                    walkable,
                    member.position,
                    member.goal,
                    &mut self.waypoints,
                );
                answer(member, &self.waypoints, outcome.reached);
                self.waiting[index].answered = true;
                continue;
            }
            if self.plan_party(&mut search, run, answer) == Planning::Spent {
                break;
            }
        }
    }

    /// Reads `asks`, those of one tick's plan, and the places of the parties' among them.
    pub(crate) fn read(&mut self, asks: impl IntoIterator<Item = Ask>) {
        self.waiting.clear();
        self.waiting.extend(asks.into_iter().map(|ask| Waiting {
            ask,
            answered: false,
        }));
        self.waiting
            .sort_unstable_by_key(|waiting| (waiting.ask.tick, waiting.ask.member.id));
        self.parties.clear();
        let place = |index: usize| u32::try_from(index).expect("asks fit a u32");
        self.parties.extend(
            (0..self.waiting.len())
                .filter(|&index| self.waiting[index].ask.party.is_some())
                .map(place),
        );
        let waiting = &self.waiting;
        self.parties.sort_unstable_by_key(|&index| {
            let ask = waiting[index as usize].ask;
            (ask.party.expect("an ask of a party").order(), ask.member.id)
        });
    }

    /// The range of `parties` that holds the asks of `party`.
    fn party_run(&self, party: Party) -> Range<usize> {
        let key = party.order();
        let of = |&index: &u32| {
            let ask = self.waiting[index as usize].ask;
            ask.party.expect("an ask of a party").order()
        };
        let start = self.parties.partition_point(|index| of(index) < key);
        let end = self.parties.partition_point(|index| of(index) <= key);
        start..end
    }

    /// Plans the routes of the party whose asks are `run` of `parties`, two at least, by stable
    /// id, and marks each it answers.
    fn plan_party(
        &mut self,
        search: &mut RouteSearch<'_>,
        run: Range<usize>,
        answer: &mut impl FnMut(Member, &[Vec3], bool),
    ) -> Planning {
        let party = self.waiting[self.parties[run.start] as usize]
            .ask
            .party
            .expect("an ask of a party");
        self.members.clear();
        for &index in &self.parties[run.clone()] {
            self.members.push(self.waiting[index as usize].ask.member);
        }
        let mut answered = 0;
        let mut search = PartySearch { search, party };
        let planning = self.plan_members(&mut search, &mut |member, waypoints, reached| {
            answered += 1;
            answer(member, waypoints, reached);
        });
        for &index in &self.parties[run.start..run.start + answered] {
            self.waiting[index as usize].answered = true;
        }
        planning
    }

    /// Plans the routes of the party's members, which `members` holds: one search for each layer
    /// they move on, which each member then takes from the last waypoint it sees. After each
    /// search its first member's tests run; before each other member's, a spent tick stops, and
    /// the rest wait.
    fn plan_members(
        &mut self,
        search: &mut PartySearch<'_, '_>,
        answer: &mut impl FnMut(Member, &[Vec3], bool),
    ) -> Planning {
        debug_assert!(self.members.len() >= 2);
        debug_assert!(self.members.is_sorted_by_key(|member| member.id));
        let center = GroupBox::of(self.members.iter().map(|member| member.position))
            .expect("a party has members")
            .center();
        self.shared.clear();
        self.layers.clear();
        for index in 0..self.members.len() {
            let member = self.members[index];
            let searched = !self
                .layers
                .iter()
                .any(|shared| shared.layer == member.walker.layer);
            if searched {
                self.search(search, member.walker.layer, center);
            } else if search.search.planner.is_spent() {
                return Planning::Spent;
            }
            self.follow(search, member, answer);
        }
        Planning::Goes
    }

    /// Plans the shared route of the members on `layer` to the party's goal, for the widest of
    /// their walkers, from the member of that walker nearest `center`, the lower stable id on a
    /// tie.
    fn search(&mut self, search: &mut PartySearch<'_, '_>, layer: Layer, center: [Num; 2]) {
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
        let distance =
            |position: Vec3| squared(position.x - center[0]) + squared(position.z - center[1]);
        let leader = on_layer
            .filter(|member| member.walker == widest)
            .min_by_key(|member| (distance(member.position), member.id))
            .expect("the widest walker is a member's");
        let goal = search.party.goal;
        let walkable = search.search.walkable(widest);
        let outcome =
            search
                .search
                .planner
                .plan(walkable, leader.position, goal, &mut self.waypoints);
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
        search: &mut PartySearch<'_, '_>,
        member: Member,
        answer: &mut impl FnMut(Member, &[Vec3], bool),
    ) {
        let PartySearch { search, party } = search;
        let shared = self
            .layers
            .iter()
            .find(|shared| shared.layer == member.walker.layer)
            .expect("its layer's route was searched")
            .clone();
        let route = &self.shared[shared.waypoints];
        let walkable = search.walkable(member.walker);
        let planner = &mut *search.planner;
        let seen = (0..route.len())
            .rev()
            .find(|&at| planner.sees(walkable, Segment::new(member.position, route[at])));
        let Some(first) = seen else {
            let outcome = planner.plan(walkable, member.position, member.goal, &mut self.waypoints);
            answer(member, &self.waypoints, outcome.reached);
            return;
        };
        self.waypoints.clear();
        self.waypoints.extend_from_slice(&route[first..]);
        let mut reached = shared.reached && member.goal == party.goal;
        if shared.reached && member.goal != party.goal {
            let before = match route.len() - first {
                1 => member.position,
                _ => route[route.len() - 2],
            };
            if planner.sees(walkable, Segment::new(before, member.goal)) {
                *self.waypoints.last_mut().expect("a seen waypoint") = member.goal;
                reached = true;
            }
        }
        answer(member, &self.waypoints, reached);
    }
}
