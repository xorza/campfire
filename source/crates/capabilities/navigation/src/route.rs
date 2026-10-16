use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Vec3;
use campfire_sim::{Position, SimComponent};
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_units::{ArrivedShort, Facing, UnitEntity};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::party::Party;
use crate::pathing_grid::PathingGrid;
use crate::progress::Progress;

/// A walker's long route to its destination: the goal it serves, with its ask while it waits for
/// the planner, the waypoints of the route planned last, the next one first among those left,
/// and where the leg to the next began. A unit with no destination has no goal, and so no ask.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[require(ArrivedShort)]
pub struct Route {
    goal: Option<Goal>,
    waypoints: Vec<Vec3>,
    next: u32,
    /// Whether the last waypoint is the goal, not the nearest place to it the walker reaches.
    reached: bool,
    /// Where the leg to the next waypoint began: the walker's place as the route or its last
    /// detour began, or the waypoint it passed last; none before its first route.
    from: Option<Vec3>,
}

/// Where a route goes, and its ask while it waits for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Goal {
    pos: Vec3,
    asked: Option<Ask>,
}

/// An ask for a route: the tick it was first asked in, and the party whose route search it
/// shares, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Ask {
    tick: Tick,
    party: Option<Party>,
}

impl Route {
    pub fn goal(&self) -> Option<Vec3> {
        self.goal.map(|goal| goal.pos)
    }

    pub fn asked(&self) -> Option<Tick> {
        self.goal.and_then(|goal| goal.asked).map(|ask| ask.tick)
    }

    /// The party whose route search the waiting ask shares.
    pub(crate) fn party(&self) -> Option<Party> {
        self.goal?.asked?.party
    }

    pub const fn reached(&self) -> bool {
        self.reached
    }

    /// Whether the walker arrived where its route ends short of its goal: no ask waits, no
    /// waypoint is left, and the last was not the goal. It stays so until it asks again.
    pub(crate) fn arrived_short(&self) -> bool {
        self.goal.is_some_and(|goal| goal.asked.is_none())
            && !self.reached
            && self.ahead().is_empty()
    }

    /// Asks the planner in `tick` for a route to `goal`, as one of `party` when it has one; the
    /// walker keeps to the route it has until the planner answers. An ask while one waits changes
    /// the goal and the party and keeps the first tick, so a walker whose goal moves each tick
    /// keeps its place among the routes that wait.
    pub fn ask(&mut self, goal: Vec3, tick: Tick, party: Option<Party>) {
        let tick = self.asked().unwrap_or(tick);
        self.goal = Some(Goal {
            pos: goal,
            asked: Some(Ask { tick, party }),
        });
    }

    /// Asks in `tick` for a route to `goal` again, if there is one, and forgets `progress`: a
    /// unit a forced move left elsewhere walks to it from there.
    pub(crate) fn ask_again(&mut self, goal: Option<Vec3>, progress: &mut Progress, tick: Tick) {
        if let Some(goal) = goal {
            self.ask(goal, tick, None);
            progress.restart();
        }
    }

    /// Takes the planner's answer to its ask: `waypoints`, the last the goal when `reached`, from
    /// the walker's place `from`.
    pub(crate) fn answer(&mut self, from: Vec3, waypoints: &[Vec3], reached: bool) {
        let goal = self
            .goal
            .as_mut()
            .expect("an answer to an ask, which has a goal");
        goal.asked = None;
        self.waypoints.clear();
        self.waypoints.extend_from_slice(waypoints);
        self.next = 0;
        self.reached = reached;
        self.from = Some(from);
    }

    /// Serves `goal` in place of the goal the route reaches, as its last waypoint.
    pub(crate) fn move_goal(&mut self, goal: Vec3) {
        debug_assert!(self.reached && self.asked().is_none());
        let last = self
            .waypoints
            .last_mut()
            .expect("a route that reaches its goal");
        *last = goal;
        self.goal = Some(Goal {
            pos: goal,
            asked: None,
        });
    }

    /// Forgets the goal and the route, and keeps the buffer of waypoints.
    pub fn clear(&mut self) {
        self.goal = None;
        self.waypoints.clear();
        self.next = 0;
        self.reached = false;
        self.from = None;
    }

    /// The waypoints the walker has yet to reach, the next first.
    pub fn ahead(&self) -> &[Vec3] {
        &self.waypoints[self.next as usize..]
    }

    /// Goes by `short` in place of the next `skipped` waypoints, from the walker's place `from`.
    /// When it replaces the last, the route ends on the goal only if `reached`: if `short` ends
    /// on it.
    pub(crate) fn splice(&mut self, from: Vec3, short: &[Vec3], skipped: usize, reached: bool) {
        let ahead = self.next as usize;
        if ahead + skipped == self.waypoints.len() {
            self.reached = reached;
        }
        self.waypoints
            .splice(..ahead + skipped, short.iter().copied());
        self.next = 0;
        self.from = Some(from);
    }

    /// Marks the next waypoint reached, where the next leg begins.
    pub(crate) fn advance(&mut self) {
        debug_assert!(!self.ahead().is_empty());
        self.from = Some(self.waypoints[self.next as usize]);
        self.next += 1;
    }

    /// The way the walker faces on its leg to the next waypoint: from where the leg began to the
    /// waypoint, so it holds for the whole leg, whatever each step's rounding (design 17, D2);
    /// past a leg of no length, the next leg's. None with no waypoint ahead off where it stands.
    pub fn way(&self) -> Option<Facing> {
        let mut from = self.from?;
        for waypoint in self.ahead() {
            let to = *waypoint;
            if let Some(way) = Facing::along([to.x - from.x, to.z - from.z]) {
                return Some(way);
            }
            from = to;
        }
        None
    }
}

impl SimComponent for Route {
    const NAME: &'static str = "navigation.route";
    type Host = UnitEntity;

    // Its decode keeps the next waypoint within the route; a route is planned over the cells of
    // its unit's kind of walker, which must be one the mode has; each of its places lies within
    // the world's bound, as every place does.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let within = |point: Vec3| Position::from(point).within_bound();
        let goal = |goal: Goal| {
            let party = goal.asked.and_then(|ask| ask.party);
            within(goal.pos) && party.is_none_or(|party| within(party.goal))
        };
        let places = self.goal.is_none_or(goal)
            && self.from.is_none_or(within)
            && self.waypoints.iter().copied().all(within);
        PathingGrid::serves(world, entity) && places
    }
}

impl Replication for Route {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}

/// A snapshot is untrusted, so a next waypoint past the last, or waypoints ahead with no place
/// their leg began, fail to decode.
impl<'de> Deserialize<'de> for Route {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            goal: Option<Goal>,
            waypoints: Vec<Vec3>,
            next: u32,
            reached: bool,
            from: Option<Vec3>,
        }
        let Fields {
            goal,
            waypoints,
            next,
            reached,
            from,
        } = Fields::deserialize(deserializer)?;
        if next as usize > waypoints.len() {
            return Err(D::Error::custom("a route's next waypoint past its last"));
        }
        if from.is_none() && (next as usize) < waypoints.len() {
            return Err(D::Error::custom("a route's leg with no start"));
        }
        Ok(Self {
            goal,
            waypoints,
            next,
            reached,
            from,
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;
    use campfire_math::Num;

    use super::*;

    #[test]
    fn a_route_faces_each_legs_way_from_where_the_leg_began() {
        let at = |x: i16, z: i16| Vec3::from((x, 0, z));
        let degrees = |route: &Route| route.way().map(Facing::degrees);
        let mut route = Route::default();
        assert_eq!(degrees(&route), None);
        // From (0, 0), to (0, 0) again, then (4, 0), then (4, −3): the leg of no length is passed
        // over, so the way is along `x`, 0°.
        route.ask(at(4, -3), Tick::new(0), None);
        route.answer(at(0, 0), &[at(0, 0), at(4, 0), at(4, -3)], true);
        assert_eq!(degrees(&route), Some(Num::ZERO));
        // Past (0, 0) and (4, 0), the leg from (4, 0) to (4, −3) is along `−z`, 90°, wherever
        // the walker stands on it.
        route.advance();
        route.advance();
        assert_eq!(degrees(&route), Some(Num::int(90)));
        // A detour from (5, −1) to (4, −1), along `−x`, 180°, in place of the next waypoint.
        route.splice(at(5, -1), &[at(4, -1)], 0, false);
        assert_eq!(degrees(&route), Some(Num::int(180)));
        // At the end, none; cleared, none.
        route.advance();
        route.advance();
        assert_eq!(degrees(&route), None);
        route.clear();
        assert_eq!(degrees(&route), None);
    }

    #[test]
    fn a_route_of_waypoints_ahead_and_no_leg_start_does_not_decode() {
        let mut route = Route::default();
        route.ask(Vec3::from((1, 0, 0)), Tick::new(0), None);
        route.answer(Vec3::from((0, 0, 0)), &[Vec3::from((1, 0, 0))], true);
        assert_eq!(
            Binary::decode::<Route>(&Binary::encode(&route)).ok(),
            Some(route.clone())
        );
        route.from = None;
        assert!(Binary::decode::<Route>(&Binary::encode(&route)).is_err());
        // Past its last waypoint, it needs none.
        route.next = 1;
        assert!(Binary::decode::<Route>(&Binary::encode(&route)).is_ok());
    }
}
