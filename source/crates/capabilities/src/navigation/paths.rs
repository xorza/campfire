use std::ops::Range;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_sim::Position;

use crate::navigation::path_walker::PathEnd;
use crate::units::path_id::PathId;

/// The map's paths, each a named list of waypoints that walkers go along forward or backward. Map
/// data, not state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct Paths {
    /// Shared with the script view, which names paths to scripts.
    names: Arc<[Box<str>]>,
    points: Vec<Position>,
    /// Each path's waypoints in `points`.
    ranges: Vec<Range<u32>>,
}

impl Paths {
    /// Paths of `(name, path)`, each path of at least one waypoint, each name its own.
    pub fn new<'a>(paths: impl IntoIterator<Item = (&'a str, &'a [Position])>) -> Paths {
        let mut names: Vec<Box<str>> = Vec::new();
        let mut points = Vec::new();
        let mut ranges = Vec::new();
        for (name, path) in paths {
            assert!(!path.is_empty(), "a path has a waypoint");
            assert!(
                names.iter().all(|held| **held != *name),
                "a path's name is its own"
            );
            names.push(name.into());
            let start = u32::try_from(points.len()).expect("waypoints fit u32");
            points.extend_from_slice(path);
            let end = u32::try_from(points.len()).expect("waypoints fit u32");
            ranges.push(start..end);
        }
        Paths {
            names: names.into(),
            points,
            ranges,
        }
    }

    /// Each path's name, by path.
    pub fn names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.names.iter().map(|name| &**name)
    }

    /// The names, shared with a reader of them outside the world.
    pub(crate) fn shared_names(&self) -> Arc<[Box<str>]> {
        Arc::clone(&self.names)
    }

    pub fn count(&self) -> u32 {
        u32::try_from(self.ranges.len()).expect("paths fit u32")
    }

    /// The path named `name`.
    pub fn named(&self, name: &str) -> Option<PathId> {
        let at = self.names.iter().position(|held| **held == *name)?;
        Some(PathId::new(at))
    }

    pub fn name(&self, path: PathId) -> &str {
        &self.names[path.index()]
    }

    /// Waypoint `index` of `path` counted from its end `from`; `None` past the last.
    pub fn waypoint(&self, path: PathId, index: u32, from: PathEnd) -> Option<Position> {
        let range = self.ranges.get(path.index())?;
        let len = range.end - range.start;
        let at = match from {
            PathEnd::Start => index,
            PathEnd::End => len.checked_sub(1)?.checked_sub(index)?,
        };
        (at < len).then(|| self.points[(range.start + at) as usize])
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::{Num, Vec3};

    use super::*;

    fn at(x: i64, y: i64, z: i64) -> Position {
        Position::new(Vec3::new(Num::int(x), Num::int(y), Num::int(z))).unwrap()
    }

    #[test]
    fn paths_count_waypoints_in_either_direction() {
        let paths = Paths::new([
            ("near", &[at(0, 0, 0), at(1, 0, 0)][..]),
            ("far", &[at(5, 0, 5)][..]),
        ]);
        assert_eq!(paths.count(), 2);
        assert_eq!(
            (paths.named("far"), paths.named("none")),
            (Some(PathId::new(1)), None)
        );
        assert_eq!(
            (paths.name(PathId::new(0)), paths.name(PathId::new(1))),
            ("near", "far")
        );
        let walk = |path, direction| {
            (0..3)
                .map(|index| paths.waypoint(path, index, direction))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            walk(PathId::new(0), PathEnd::Start),
            [Some(at(0, 0, 0)), Some(at(1, 0, 0)), None]
        );
        assert_eq!(
            walk(PathId::new(0), PathEnd::End),
            [Some(at(1, 0, 0)), Some(at(0, 0, 0)), None]
        );
        assert_eq!(
            walk(PathId::new(1), PathEnd::End),
            [Some(at(5, 0, 5)), None, None]
        );
        assert_eq!(walk(PathId::new(2), PathEnd::Start), [None, None, None]);
    }
}
