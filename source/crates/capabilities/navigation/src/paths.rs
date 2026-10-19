use std::ops::Range;
use std::sync::Arc;

use campfire_math::Vec3;
use campfire_values::{NameId, Names, PathId};

use crate::path_walk::PathWalk;
use crate::path_walker::PathEnd;

/// The map's paths, each a named list of waypoints that walkers go along forward or backward,
/// and how they go past its last.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Paths {
    /// Shared with the script view, which names paths to scripts.
    names: Arc<Names<PathId>>,
    points: Vec<Vec3>,
    /// Each path's waypoints in `points`.
    ranges: Vec<Range<u32>>,
    walks: Vec<PathWalk>,
}

/// A path as the map gives it: its waypoints from its start, one at the least, and how its
/// walkers go past the last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathOf {
    pub(crate) points: Vec<Vec3>,
    pub(crate) walk: PathWalk,
}

impl Paths {
    /// The paths named `names`, by id, of `paths`.
    pub(crate) fn of(names: Names<PathId>, paths: &[PathOf]) -> Self {
        assert_eq!(names.len(), paths.len(), "a path has a name");
        let count = paths.iter().map(|path| path.points.len()).sum();
        let mut points = Vec::with_capacity(count);
        let mut ranges = Vec::with_capacity(paths.len());
        for path in paths {
            assert!(!path.points.is_empty(), "a path has a waypoint");
            let start = u32::try_from(points.len()).expect("waypoints fit u32");
            points.extend_from_slice(&path.points);
            let end = u32::try_from(points.len()).expect("waypoints fit u32");
            ranges.push(start..end);
        }
        Self {
            names: Arc::new(names),
            points,
            ranges,
            walks: paths.iter().map(|path| path.walk).collect(),
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
    pub fn points(&self, path: PathId) -> &[Vec3] {
        let range = &self.ranges[path.index()];
        &self.points[range.start as usize..range.end as usize]
    }

    /// How the walkers of `path` go past its last waypoint.
    pub fn walk(&self, path: PathId) -> PathWalk {
        self.walks[path.index()]
    }

    /// Waypoint `index` of `path` counted from its end `from`; `None` past the last.
    pub fn waypoint(&self, path: PathId, index: u32, from: PathEnd) -> Option<Vec3> {
        let range = self.ranges.get(path.index())?;
        let len = range.end - range.start;
        let at = match from {
            PathEnd::Start => index,
            PathEnd::End => len.checked_sub(1)?.checked_sub(index)?,
        };
        (at < len).then(|| self.points[(range.start + at) as usize])
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Vec3;
    use campfire_values::Names;

    use crate::path_walk::PathWalk;
    use crate::paths::{PathOf, Paths};

    impl Paths {
        /// Paths of `(name, path)`, each walked once, each path of at least one waypoint, each
        /// name its own.
        pub fn new<'a>(paths: impl IntoIterator<Item = (&'a str, &'a [Vec3])>) -> Self {
            Self::walked(
                paths
                    .into_iter()
                    .map(|(name, path)| (name, path, PathWalk::Once)),
            )
        }

        /// Paths of `(name, path, walk)`, as `new` takes them, each walked as `walk` says.
        pub fn walked<'a>(
            paths: impl IntoIterator<Item = (&'a str, &'a [Vec3], PathWalk)>,
        ) -> Self {
            let mut names = Names::default();
            let mut of = Vec::new();
            for (name, points, walk) in paths {
                names.add(name);
                of.push(PathOf {
                    points: points.to_vec(),
                    walk,
                });
            }
            Self::of(names, &of)
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn paths_count_waypoints_in_either_direction() {
        let paths = Paths::new([
            ("near", &[Vec3::from((0, 0, 0)), Vec3::from((1, 0, 0))][..]),
            ("far", &[Vec3::from((5, 0, 5))][..]),
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
                Some(Vec3::from((0, 0, 0))),
                Some(Vec3::from((1, 0, 0))),
                None
            ]
        );
        assert_eq!(
            walk(PathId::new(0), PathEnd::End),
            [
                Some(Vec3::from((1, 0, 0))),
                Some(Vec3::from((0, 0, 0))),
                None
            ]
        );
        assert_eq!(
            walk(PathId::new(1), PathEnd::End),
            [Some(Vec3::from((5, 0, 5))), None, None]
        );
        assert_eq!(walk(PathId::new(2), PathEnd::Start), [None, None, None]);
    }
}
