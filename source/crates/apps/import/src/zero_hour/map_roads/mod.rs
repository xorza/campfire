use std::collections::BTreeMap;

use bevy_math::Vec2;

use crate::zero_hour::dropped_roads::{DroppedRoads, RoadDropCause};
use crate::zero_hour::format::ini::roads_ini::{RoadId, RoadsIni};
use crate::zero_hour::format::map::map_object::MapObject;

/// A map's roads, as `W3DRoadBuffer::addMapObjects` reads them from its objects, in `f32` as the
/// game computes them: each object that starts a road and the one after it, when that one ends
/// it, a segment; those of a type `Roads.ini` names, in the map's order, the types they are, each
/// once, in the order the game draws them, the last read first, and the count of those of no type
/// by their template, which the game draws only in a build of test assets (roads.md, A road's
/// type is `Roads.ini`'s).
#[derive(Debug)]
pub(crate) struct MapRoads {
    segments: Vec<RoadSegment>,
    types: Vec<RoadId>,
    untyped: BTreeMap<Vec<u8>, usize>,
}

/// A segment of road: its type, its start and its end, and whether a curve at its start is
/// tight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RoadSegment {
    pub(crate) road: RoadId,
    pub(crate) start: RoadEnd,
    pub(crate) end: RoadEnd,
    pub(crate) tight: bool,
}

/// An end of a segment: its point on the game's ground, whether the corner there is a miter, and
/// whether it asks for an alpha join to a road of another type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RoadEnd {
    pub(crate) point: Vec2,
    pub(crate) angled: bool,
    pub(crate) join: bool,
}

/// How far the game moves a segment's end along `x` off its start's point when the two meet
/// (`addMapObjects`).
const APART: f32 = 0.25;

impl MapRoads {
    /// The roads of `objects`, a map's, of the types of `roads`.
    pub(crate) fn of(objects: &[MapObject], roads: &RoadsIni) -> Self {
        let mut segments = Vec::new();
        let mut untyped = BTreeMap::new();
        let mut at = 0;
        while let Some(start) = objects.get(at) {
            at += 1;
            if !start.is_road_start() {
                continue;
            }
            let Some(end) = objects.get(at) else {
                break;
            };
            if !end.is_road_end() {
                continue;
            }
            at += 1;
            let Some(road) = roads.road(&start.template) else {
                *untyped.entry(start.template.clone()).or_insert(0) += 1;
                continue;
            };
            let point = |object: &MapObject| Vec2::new(object.position[0], object.position[1]);
            let [from, mut to] = [point(start), point(end)];
            if from == to {
                to.x += APART;
            }
            segments.push(RoadSegment {
                road,
                start: RoadEnd {
                    point: from,
                    angled: start.is_corner_angled(),
                    join: start.is_road_join(),
                },
                end: RoadEnd {
                    point: to,
                    angled: end.is_corner_angled(),
                    join: end.is_road_join(),
                },
                tight: start.is_corner_tight(),
            });
        }
        let mut types: Vec<RoadId> = segments.iter().map(|segment| segment.road).collect();
        types.sort_unstable_by(|a, b| b.cmp(a));
        types.dedup();
        Self {
            segments,
            types,
            untyped,
        }
    }

    /// Its segments of a type, in the map's order.
    pub(crate) fn segments(&self) -> &[RoadSegment] {
        &self.segments
    }

    /// The types its segments are, each once, in the order the game draws them.
    pub(crate) fn types(&self) -> &[RoadId] {
        &self.types
    }

    /// Its segments of no type, by their template, as the import's losses name them.
    pub(crate) fn untyped(&self) -> impl Iterator<Item = DroppedRoads> {
        self.untyped.iter().map(|(template, &count)| DroppedRoads {
            road: template.iter().map(|&byte| char::from(byte)).collect(),
            cause: RoadDropCause::NoType,
            count,
        })
    }
}

#[cfg(test)]
mod tests;
