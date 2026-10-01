use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use campfire_sim::{Position, SimComponent, StableId, Tick};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

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

/// A walker whose route waits for the planner, in the order routes are planned: by the tick it
/// asked in, then by stable id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Waiting {
    pub(crate) tick: Tick,
    pub(crate) id: StableId,
    pub(crate) entity: Entity,
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

    /// Asks the planner in `tick` for a route to `goal`; the walker keeps to the route it has
    /// until the planner answers.
    pub(crate) const fn ask(&mut self, goal: Position, tick: Tick) {
        self.goal = Some(goal);
        self.asked = Some(tick);
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

    /// Forgets the goal and the route.
    pub(crate) fn clear(&mut self) {
        *self = Route::default();
    }

    /// The waypoints the walker has yet to reach, the next first.
    pub fn ahead(&self) -> &[Position] {
        &self.waypoints[self.next as usize..]
    }

    /// Marks the next waypoint reached.
    pub(crate) fn advance(&mut self) {
        debug_assert!(!self.ahead().is_empty());
        self.next += 1;
    }
}

impl SimComponent for Route {
    const NAME: &'static str = "navigation.route";
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
