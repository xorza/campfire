use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::NotSent;
use crate::values::engine_enum::EngineEnum;
use crate::values::script_enum::ScriptEnum;

/// A unit walking the path its `OnPath` names, such as a creep: it walks to the path's waypoints
/// from the end it starts at while it has no other order, and stays at the last. It keeps the
/// spawn group it came from, whose units on their path share their route searches.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathWalker {
    from: PathEnd,
    /// The waypoint it walks to next, counted from its end.
    next: u32,
    /// Whether it left the path for a place it was ordered to, until it is told to walk it again.
    left: bool,
    /// The stable id of its spawn group's first unit; a placed unit's own.
    group: StableId,
}

/// The end of a path a unit walks it from, as a placed unit's or a spawn group's `from` names it:
/// a MOBA's two sides walk each path from their own end, and a third team either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathEnd {
    Start,
    End,
}

/// `PathEnd::Start` and `PathEnd::End` in scripts, `start` and `end` in data.
impl ScriptEnum for PathEnd {
    const ENUM: EngineEnum = EngineEnum::PathEnd;
    const MEMBERS: &'static [(&'static str, PathEnd)] =
        &[("Start", PathEnd::Start), ("End", PathEnd::End)];

    fn data_name(self) -> &'static str {
        match self {
            PathEnd::Start => "start",
            PathEnd::End => "end",
        }
    }
}

impl PathWalker {
    /// A walker of `group` at the end `from` of its path, bound for its second waypoint from
    /// there.
    pub const fn start(from: PathEnd, group: StableId) -> PathWalker {
        PathWalker {
            from,
            next: 1,
            left: false,
            group,
        }
    }

    pub const fn walks_from(self) -> PathEnd {
        self.from
    }

    pub(crate) const fn group(self) -> StableId {
        self.group
    }

    pub const fn next(self) -> u32 {
        self.next
    }

    pub(crate) const fn advance(&mut self) {
        self.next += 1;
    }

    /// Whether it left the path, and walks it no more until it rejoins it.
    pub const fn left(self) -> bool {
        self.left
    }

    pub(crate) const fn leave(&mut self) {
        self.left = true;
    }

    pub(crate) const fn rejoin(&mut self) {
        self.left = false;
    }
}

impl SimComponent for PathWalker {
    const NAME: &'static str = "navigation.path_walker";

    // Its path's waypoints are read with a check, and one past the last ends the walk.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for PathWalker {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::script_enum::internals::named_as_data;

    #[test]
    fn each_path_end_is_named_in_scripts_as_data_names_it() {
        named_as_data::<PathEnd>();
    }
}
