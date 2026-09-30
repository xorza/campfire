use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use campfire_sim::{Position, SimComponent, StableId, Tick};
use serde::{Deserialize, Serialize};

/// A walker's long route: the goal it asked a route to, while the route waits for the planner,
/// and the waypoints of the route planned last, the last the goal or the nearest place to it the
/// walker reaches.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Route {
    asked: Option<RouteAsk>,
    waypoints: Vec<Position>,
}

/// A goal a walker asked a route to, and the tick it asked in: routes are planned in the order
/// they were asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RouteAsk {
    pub(crate) goal: Position,
    pub(crate) tick: Tick,
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
    /// Asks the planner for a route to `goal` in `tick`; the route planned before stays until the
    /// planner answers.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the plan's next step makes orders ask routes")
    )]
    pub(crate) const fn ask(&mut self, goal: Position, tick: Tick) {
        self.asked = Some(RouteAsk { goal, tick });
    }

    pub(crate) const fn asked(&self) -> Option<RouteAsk> {
        self.asked
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the plan's next step walks the waypoints")
    )]
    pub(crate) const fn waypoints(&self) -> &[Position] {
        self.waypoints.as_slice()
    }

    /// Takes the ask away, and hands the waypoints, cleared, to the planner to fill.
    pub(crate) fn answer(&mut self) -> &mut Vec<Position> {
        self.asked = None;
        self.waypoints.clear();
        &mut self.waypoints
    }
}

impl SimComponent for Route {
    const NAME: &'static str = "navigation.route";
}
