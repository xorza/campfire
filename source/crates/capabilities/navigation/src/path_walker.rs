use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{EnumIdents, NamedEnum};
use campfire_math::Vec3;
use campfire_script::enum_script_arg;
use campfire_sim::{SimComponent, StableId};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::{EngineEnum, ScriptEnum};
use campfire_visibility::DataKind;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::party::{Party, PartyKey};
use crate::path_walk::PathWalk;

/// A unit walking the path its `OnPath` names, such as a creep: it walks to the path's waypoints
/// from the end it walks from while it has no other order, and past the last as the path's walk
/// says. It keeps the spawn group it came from, whose units on their path share their route
/// searches.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathWalker {
    /// The end it walks from, which a walk back and forth turns at each end.
    from: PathEnd,
    /// The waypoint it walks to next, counted from its end.
    next: u32,
    /// Whether it left the path for a place it was ordered to, until it is told to walk it again.
    left: bool,
    /// The stable id of its spawn group's first unit; a placed unit's own.
    group: StableId,
}

/// The end of a path a unit walks it from, as a placed unit's or a spawn group's `from`
/// names it: a MOBA's two sides walk each path from their own end, and a third team either
/// way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIdents, NamedEnum)]
pub enum PathEnd {
    Start,
    End,
}

/// Written and read as serde's derive does a unit variant: a binary format, as a snapshot, by its
/// place, and a text format, as data, by its name.
impl Serialize for PathEnd {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.serialize_member(serializer)
    }
}

impl<'de> Deserialize<'de> for PathEnd {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize_member(deserializer)
    }
}

impl PathEnd {
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Start => Self::End,
            Self::End => Self::Start,
        }
    }
}

/// `PathEnd::Start` and `PathEnd::End` in scripts.
impl ScriptEnum for PathEnd {
    const ENUM: EngineEnum = EngineEnum::PathEnd;
    const SCRIPT_NAMES: &'static [&'static str] = &Self::IDENTS;
}

impl PathWalker {
    /// A walker of `group` at the end `from` of its path, bound for its second waypoint from
    /// there.
    pub const fn start(from: PathEnd, group: StableId) -> Self {
        Self {
            from,
            next: 1,
            left: false,
            group,
        }
    }

    pub const fn walks_from(self) -> PathEnd {
        self.from
    }

    /// The party it asks a route to `goal` as: its spawn group's, unless it left its path.
    pub(crate) const fn party(self, goal: Vec3) -> Option<Party> {
        if self.left {
            return None;
        }
        Some(Party {
            key: PartyKey::Spawn(self.group),
            goal,
        })
    }

    pub const fn next(self) -> u32 {
        self.next
    }

    /// Goes on from the waypoint it reached to the next of a path of `len` waypoints, walked as
    /// `walk` says: whether the waypoint was its last, from the end it walks from. Past the last,
    /// a walk once ends, a loop goes to the first again, and a walk back and forth turns and
    /// walks from the other end, to the waypoint before the last.
    pub(crate) const fn advance(&mut self, len: u32, walk: PathWalk) -> bool {
        debug_assert!(self.next < len, "a walker reaches a waypoint of its path");
        if self.next + 1 < len {
            self.next += 1;
            return false;
        }
        match walk {
            PathWalk::Once => self.next = len,
            PathWalk::Loop => self.next = 0,
            PathWalk::BackAndForth => {
                self.from = self.from.other();
                self.next = 1;
            }
        }
        true
    }

    /// Whether it left the path, and walks it no more until it rejoins it.
    pub const fn is_left(self) -> bool {
        self.left
    }

    pub const fn leave(&mut self) {
        self.left = true;
    }

    pub const fn rejoin(&mut self) {
        self.left = false;
    }
}

impl SimComponent for PathWalker {
    const NAME: &'static str = "navigation.path_walker";
    type Host = UnitEntity;

    // Its path's waypoints are read with a check, and one past the last ends the walk.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for PathWalker {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}

enum_script_arg!(PathEnd);

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;
    use campfire_values::internals::named_as_data;

    use super::*;

    #[test]
    fn each_path_end_is_named_in_scripts_as_data_names_it() {
        named_as_data::<PathEnd>();
    }

    #[test]
    fn a_walker_goes_past_its_last_waypoint_as_its_path_is_walked() {
        // On a path of 3 waypoints from its start, a walker bound for waypoint 1 reaches it and
        // goes to 2, which is the last: a walk once ends, 3 being past the last; a loop goes to
        // 0; a walk back and forth walks from the end, to its waypoint 1, the middle.
        let group = IdAllocator::default().allocate();
        let walked = |walk| {
            let mut walker = PathWalker::start(PathEnd::Start, group);
            let ends = [walker.advance(3, walk), walker.advance(3, walk)];
            (ends, walker.next(), walker.walks_from())
        };
        let on = [false, true];
        assert_eq!(walked(PathWalk::Once), (on, 3, PathEnd::Start));
        assert_eq!(walked(PathWalk::Loop), (on, 0, PathEnd::Start));
        assert_eq!(walked(PathWalk::BackAndForth), (on, 1, PathEnd::End));
        assert_eq!(PathEnd::Start.other().other(), PathEnd::Start);
    }
}
