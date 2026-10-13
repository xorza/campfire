use std::ops::Range;
use std::sync::Arc;

use campfire_sim::Position;

use crate::navigation::path_walker::PathEnd;
use crate::units::path_id::PathId;
use crate::values::names::Names;

/// The map's paths, each a named list of waypoints that walkers go along forward or backward.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Paths {
    /// Shared with the script view, which names paths to scripts.
    names: Arc<Names<PathId>>,
    points: Vec<Position>,
    /// Each path's waypoints in `points`.
    ranges: Vec<Range<u32>>,
}

impl Paths {
    /// The paths named `names`, by id, of the waypoints `paths`, each of one at the least.
    pub(crate) fn of(names: Names<PathId>, paths: &[Vec<Position>]) -> Paths {
        assert_eq!(names.len(), paths.len(), "a path has a name");
        let mut points = Vec::with_capacity(paths.iter().map(Vec::len).sum());
        let mut ranges = Vec::with_capacity(paths.len());
        for path in paths {
            assert!(!path.is_empty(), "a path has a waypoint");
            let start = u32::try_from(points.len()).expect("waypoints fit u32");
            points.extend_from_slice(path);
            let end = u32::try_from(points.len()).expect("waypoints fit u32");
            ranges.push(start..end);
        }
        Paths {
            names: Arc::new(names),
            points,
            ranges,
        }
    }

    /// The paths' names.
    pub fn names(&self) -> &Names<PathId> {
        &self.names
    }

    /// The names, shared with a reader of them outside the world.
    pub(crate) fn shared_names(&self) -> Arc<Names<PathId>> {
        Arc::clone(&self.names)
    }

    pub fn count(&self) -> u32 {
        u32::try_from(self.ranges.len()).expect("paths fit u32")
    }

    pub fn name(&self, path: PathId) -> &str {
        self.names.name(path).expect("a path of the map")
    }

    /// The waypoints of `path`, from its start.
    pub(crate) fn points(&self, path: PathId) -> &[Position] {
        let range = &self.ranges[path.index()];
        &self.points[range.start as usize..range.end as usize]
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
pub(crate) mod internals {
    use campfire_sim::Position;

    use crate::navigation::paths::Paths;
    use crate::values::names::Names;

    impl Paths {
        /// Paths of `(name, path)`, each path of at least one waypoint, each name its own.
        pub(crate) fn new<'a>(paths: impl IntoIterator<Item = (&'a str, &'a [Position])>) -> Paths {
            let mut names = Names::default();
            let mut points = Vec::new();
            for (name, path) in paths {
                names.add(name);
                points.push(path.to_vec());
            }
            Paths::of(names, &points)
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn paths_count_waypoints_in_either_direction() {
        let paths = Paths::new([
            (
                "near",
                &[Position::from((0, 0, 0)), Position::from((1, 0, 0))][..],
            ),
            ("far", &[Position::from((5, 0, 5))][..]),
        ]);
        assert_eq!(paths.count(), 2);
        assert_eq!(
            (paths.names().named("far"), paths.names().named("none")),
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
            [
                Some(Position::from((0, 0, 0))),
                Some(Position::from((1, 0, 0))),
                None
            ]
        );
        assert_eq!(
            walk(PathId::new(0), PathEnd::End),
            [
                Some(Position::from((1, 0, 0))),
                Some(Position::from((0, 0, 0))),
                None
            ]
        );
        assert_eq!(
            walk(PathId::new(1), PathEnd::End),
            [Some(Position::from((5, 0, 5))), None, None]
        );
        assert_eq!(walk(PathId::new(2), PathEnd::Start), [None, None, None]);
    }
}
