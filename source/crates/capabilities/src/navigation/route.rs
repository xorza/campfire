use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Tick;
use campfire_sim::{Position, SimComponent};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::navigation::pathing_grid::PathingGrid;

/// A walker's long route to its destination: the goal it serves, the tick it asked the planner
/// for a route there while it waits for one, and the waypoints of the route planned last, the
/// next one first among those left. A unit with no destination has no goal.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Route {
    goal: Option<Position>,
    asked: Option<Tick>,
    waypoints: Vec<Position>,
    next: u32,
    /// Whether the last waypoint is the goal, not the nearest place to it the walker reaches.
    reached: bool,
}

impl Route {
    pub(crate) const fn goal(&self) -> Option<Position> {
        self.goal
    }

    pub(crate) const fn asked(&self) -> Option<Tick> {
        self.asked
    }

    pub(crate) const fn reached(&self) -> bool {
        self.reached
    }

    /// Whether the walker arrived where its route ends short of its goal: no ask waits, no
    /// waypoint is left, and the last was not the goal. It stays so until it asks again.
    pub(crate) fn arrived_short(&self) -> bool {
        self.goal.is_some() && self.asked.is_none() && !self.reached && self.ahead().is_empty()
    }

    /// Whether it arrived short of `goal`.
    pub(crate) fn arrived_short_of(&self, goal: Position) -> bool {
        self.goal == Some(goal) && self.arrived_short()
    }

    /// Asks the planner in `tick` for a route to `goal`; the walker keeps to the route it has
    /// until the planner answers. An ask while one waits changes the goal and keeps the first
    /// tick, so a walker whose goal moves each tick keeps its place among the routes that wait.
    pub(crate) const fn ask(&mut self, goal: Position, tick: Tick) {
        self.goal = Some(goal);
        if self.asked.is_none() {
            self.asked = Some(tick);
        }
    }

    /// Takes the planner's answer: `waypoints`, the last the goal when `reached`.
    pub(crate) fn answer(&mut self, waypoints: &[Position], reached: bool) {
        self.asked = None;
        self.waypoints.clear();
        self.waypoints.extend_from_slice(waypoints);
        self.next = 0;
        self.reached = reached;
    }

    /// Serves `goal` in place of the goal the route reaches, as its last waypoint.
    pub(crate) fn move_goal(&mut self, goal: Position) {
        debug_assert!(self.reached && self.asked.is_none());
        let last = self
            .waypoints
            .last_mut()
            .expect("a route that reaches its goal");
        *last = goal;
        self.goal = Some(goal);
    }

    /// Forgets the goal and the route, and keeps the buffer of waypoints.
    pub(crate) fn clear(&mut self) {
        self.goal = None;
        self.asked = None;
        self.waypoints.clear();
        self.next = 0;
        self.reached = false;
    }

    /// The waypoints the walker has yet to reach, the next first.
    pub fn ahead(&self) -> &[Position] {
        &self.waypoints[self.next as usize..]
    }

    /// Goes by `short` in place of the next `skipped` waypoints. When it replaces the last, the
    /// route ends on the goal only if `reached`: if `short` ends on it.
    pub(crate) fn splice(&mut self, short: &[Position], skipped: usize, reached: bool) {
        let ahead = self.next as usize;
        if ahead + skipped == self.waypoints.len() {
            self.reached = reached;
        }
        self.waypoints
            .splice(..ahead + skipped, short.iter().copied());
        self.next = 0;
    }

    /// Marks the next waypoint reached.
    pub(crate) fn advance(&mut self) {
        debug_assert!(!self.ahead().is_empty());
        self.next += 1;
    }
}

impl SimComponent for Route {
    const NAME: &'static str = "navigation.route";

    // Its decode keeps the next waypoint within the route; a route is planned over the cells of
    // its unit's kind of walker, which must be one the mode has.
    fn check(&self, world: &World, entity: Entity) -> bool {
        PathingGrid::serves(world, entity)
    }
}

/// A snapshot is untrusted, so a next waypoint past the last fails to decode.
impl<'de> Deserialize<'de> for Route {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Route, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            goal: Option<Position>,
            asked: Option<Tick>,
            waypoints: Vec<Position>,
            next: u32,
            reached: bool,
        }
        let Fields {
            goal,
            asked,
            waypoints,
            next,
            reached,
        } = Fields::deserialize(deserializer)?;
        if next as usize > waypoints.len() {
            return Err(D::Error::custom("a route's next waypoint past its last"));
        }
        Ok(Route {
            goal,
            asked,
            waypoints,
            next,
            reached,
        })
    }
}
