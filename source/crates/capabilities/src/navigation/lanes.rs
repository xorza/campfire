use std::ops::Range;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_sim::Position;

use crate::navigation::lane_walker::PathDirection;
use crate::units::lane::Lane;

/// The map's lanes, each a named path of waypoints that walkers go along forward or backward. Map
/// data, not state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct Lanes {
    /// Shared with the script view, which names lanes to scripts.
    names: Arc<[Box<str>]>,
    points: Vec<Position>,
    /// Each lane's waypoints in `points`.
    paths: Vec<Range<u32>>,
}

impl Lanes {
    /// Lanes of `(name, path)`, each path of at least one waypoint, each name its own.
    pub fn new<'a>(paths: impl IntoIterator<Item = (&'a str, &'a [Position])>) -> Lanes {
        let mut names: Vec<Box<str>> = Vec::new();
        let mut points = Vec::new();
        let mut ranges = Vec::new();
        for (name, path) in paths {
            assert!(!path.is_empty(), "a lane has a waypoint");
            assert!(
                names.iter().all(|held| **held != *name),
                "a lane's name is its own"
            );
            names.push(name.into());
            let start = u32::try_from(points.len()).expect("waypoints fit u32");
            points.extend_from_slice(path);
            let end = u32::try_from(points.len()).expect("waypoints fit u32");
            ranges.push(start..end);
        }
        Lanes {
            names: names.into(),
            points,
            paths: ranges,
        }
    }

    /// Each lane's name, by lane.
    pub fn names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.names.iter().map(|name| &**name)
    }

    /// The names, shared with a reader of them outside the world.
    pub(crate) fn shared_names(&self) -> Arc<[Box<str>]> {
        Arc::clone(&self.names)
    }

    pub fn count(&self) -> u32 {
        u32::try_from(self.paths.len()).expect("lanes fit u32")
    }

    /// The lane named `name`.
    pub fn named(&self, name: &str) -> Option<Lane> {
        let at = self.names.iter().position(|held| **held == *name)?;
        Some(Lane::new(at))
    }

    pub fn name(&self, lane: Lane) -> &str {
        &self.names[lane.index()]
    }

    /// Waypoint `index` of `lane` counted in `direction`; `None` past the last.
    pub fn waypoint(&self, lane: Lane, index: u32, direction: PathDirection) -> Option<Position> {
        let range = self.paths.get(lane.index())?;
        let len = range.end - range.start;
        let at = match direction {
            PathDirection::Forward => index,
            PathDirection::Backward => len.checked_sub(1)?.checked_sub(index)?,
        };
        (at < len).then(|| self.points[(range.start + at) as usize])
    }
}
