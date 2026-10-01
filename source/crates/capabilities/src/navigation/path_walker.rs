use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// A unit walking the path its `OnPath` names, such as a creep: it walks to the path's waypoints
/// from the end it starts at while it has no other order, and stays at the last.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathWalker {
    from: PathEnd,
    /// The waypoint it walks to next, counted from its end.
    next: u32,
}

/// The end of a path a unit walks it from, as a placed unit's or a spawn group's `from` names it:
/// a MOBA's two sides walk each path from their own end, and a third team either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathEnd {
    Start,
    End,
}

impl PathEnd {
    /// The end `name` names: `start` or `end`.
    pub fn named(name: &str) -> Option<PathEnd> {
        match name {
            "start" => Some(PathEnd::Start),
            "end" => Some(PathEnd::End),
            _ => None,
        }
    }
}

impl PathWalker {
    /// A walker at the end `from` of its path, bound for its second waypoint from there.
    pub const fn start(from: PathEnd) -> PathWalker {
        PathWalker { from, next: 1 }
    }

    pub const fn walks_from(self) -> PathEnd {
        self.from
    }

    pub const fn next(self) -> u32 {
        self.next
    }

    pub(crate) const fn advance(&mut self) {
        self.next += 1;
    }
}

impl SimComponent for PathWalker {
    const NAME: &'static str = "navigation.path_walker";
}
