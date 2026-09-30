use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_sim::Position;

use crate::team::Team;

/// The map's lanes, each a path of waypoints from the first team's base to the second's. Map
/// data, not state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct Lanes {
    points: Vec<Position>,
    /// Each lane's waypoints in `points`.
    lanes: Vec<Range<u32>>,
}

impl Lanes {
    /// Lanes along `paths`, each of at least one waypoint.
    pub fn new<'a>(paths: impl IntoIterator<Item = &'a [Position]>) -> Lanes {
        let mut lanes = Lanes::default();
        for path in paths {
            assert!(!path.is_empty(), "a lane has a waypoint");
            let start = u32::try_from(lanes.points.len()).expect("waypoints fit u32");
            lanes.points.extend_from_slice(path);
            let end = u32::try_from(lanes.points.len()).expect("waypoints fit u32");
            lanes.lanes.push(start..end);
        }
        lanes
    }

    pub fn count(&self) -> u32 {
        u32::try_from(self.lanes.len()).expect("lanes fit u32")
    }

    /// Waypoint `index` of `lane` in the order `team` walks it; `None` past the last, and for the
    /// neutral team, which walks no lane.
    pub fn waypoint(&self, lane: u32, index: u32, team: Team) -> Option<Position> {
        let range = self.lanes.get(lane as usize)?;
        let len = range.end - range.start;
        let at = match team {
            Team::First => index,
            Team::Second => len.checked_sub(1)?.checked_sub(index)?,
            Team::Neutral => return None,
        };
        (at < len).then(|| self.points[(range.start + at) as usize])
    }
}
