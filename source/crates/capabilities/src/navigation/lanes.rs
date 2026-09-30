use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_sim::Position;

use crate::navigation::lane_walker::PathDirection;

/// The map's lanes, each a named path of waypoints that walkers go along forward or backward. Map
/// data, not state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct Lanes {
    names: Vec<Box<str>>,
    points: Vec<Position>,
    /// Each lane's waypoints in `points`.
    paths: Vec<Range<u32>>,
}

impl Lanes {
    /// Lanes of `(name, path)`, each path of at least one waypoint, each name its own.
    pub fn new<'a>(paths: impl IntoIterator<Item = (&'a str, &'a [Position])>) -> Lanes {
        let mut lanes = Lanes::default();
        for (name, path) in paths {
            assert!(!path.is_empty(), "a lane has a waypoint");
            assert!(lanes.named(name).is_none(), "a lane's name is its own");
            lanes.names.push(name.into());
            let start = u32::try_from(lanes.points.len()).expect("waypoints fit u32");
            lanes.points.extend_from_slice(path);
            let end = u32::try_from(lanes.points.len()).expect("waypoints fit u32");
            lanes.paths.push(start..end);
        }
        lanes
    }

    /// Each lane's name, by lane.
    pub fn names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.names.iter().map(|name| &**name)
    }

    pub fn count(&self) -> u32 {
        u32::try_from(self.paths.len()).expect("lanes fit u32")
    }

    /// The lane named `name`.
    pub fn named(&self, name: &str) -> Option<u32> {
        let at = self.names.iter().position(|held| **held == *name)?;
        Some(u32::try_from(at).expect("lanes fit u32"))
    }

    pub fn name(&self, lane: u32) -> &str {
        &self.names[lane as usize]
    }

    /// Waypoint `index` of `lane` counted in `direction`; `None` past the last.
    pub fn waypoint(&self, lane: u32, index: u32, direction: PathDirection) -> Option<Position> {
        let range = self.paths.get(lane as usize)?;
        let len = range.end - range.start;
        let at = match direction {
            PathDirection::Forward => index,
            PathDirection::Backward => len.checked_sub(1)?.checked_sub(index)?,
        };
        (at < len).then(|| self.points[(range.start + at) as usize])
    }
}
