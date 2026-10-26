use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_6};

use crate::zero_hour::dropped_roads::{DroppedRoads, RoadDropCause};
use crate::zero_hour::format::ini::roads_ini::{RoadId, RoadsIni};
use crate::zero_hour::map_roads::{MapRoads, RoadSegment};
use crate::zero_hour::road_buffer::buffer_road::{BufferRoad, PieceKind};
use crate::zero_hour::road_decals::RoadDecals;
use crate::zero_hour::road_mesh::{RoadGround, RoadMesh};
use crate::zero_hour::road_pieces::RoadPieces;
use crate::zero_hour::terrain_atlas::TexturePlace;

/// A map's road pieces as `W3DRoadBuffer::loadRoads` builds them, in `f32` as the game computes
/// them (roads.md, A road is a decal the import meshes): its segments chained, each to one of its
/// type that shares its point, flipped to run on, so a chain runs back along the list; the ends of
/// a type at each point counted; a junction where three or four meet; a curve or a miter where
/// the ends of two segments next in a chain meet alone; and an alpha join where an end alone of
/// its type asks for one, its type stacked over the road it meets.
#[derive(Debug)]
pub(crate) struct RoadBuffer {
    roads: Vec<BufferRoad>,
    miters: usize,
    /// Each road type's stacking, of those over another, which the game draws later.
    stacks: BTreeMap<RoadId, u32>,
    /// The map's road types, in the order the game draws those of one stacking, and its segments
    /// of no type.
    types: Vec<RoadId>,
    untyped: Vec<DroppedRoads>,
}

/// A map's roads as its import draws them: their decals, the pieces they draw, and the segments
/// it drops.
#[derive(Debug)]
pub(crate) struct DrawnRoads {
    pub(crate) decals: RoadDecals,
    pub(crate) pieces: RoadPieces,
    pub(crate) dropped: Vec<DroppedRoads>,
}

/// Whether a piece added to the buffer counts the ends it meets: on the first pass, which
/// counts, or on the second, which keeps the first's counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Counting {
    Count,
    Keep,
}

/// How far along a road a curve's ends must stay of the segments' far ends, in units
/// (`DOT_LIMIT`).
const LEAST_LEFT: f32 = 0.5;
/// The turn below which, in steps of 30°, a corner is a miter.
const LEAST_CURVE: f32 = 0.9;

impl RoadBuffer {
    /// The pieces of `map`'s segments, of the types of `types`, as the game builds them: each
    /// segment added in turn, counting the ends it meets, then each added again in the list's
    /// order, keeping its counts (`addMapObjects`); the ends counted again
    /// (`updateCountsAndFlags`); the junctions inserted (`insertTeeIntersections`); the curves
    /// (`insertCurveSegments`); then the alpha joins (`insertCrossTypeJoins`).
    pub(crate) fn of(map: &MapRoads, types: &RoadsIni) -> Self {
        Self {
            types: map.types().to_vec(),
            untyped: map.untyped().collect(),
            ..Self::chained(map.segments(), types)
        }
    }

    /// The pieces of `segments`, of the types of `types`, as `of` builds them, of no map's types.
    fn chained(segments: &[RoadSegment], types: &RoadsIni) -> Self {
        let mut buffer = Self {
            roads: Vec::with_capacity(segments.len()),
            miters: 0,
            stacks: BTreeMap::new(),
            types: Vec::new(),
            untyped: Vec::new(),
        };
        for segment in segments {
            let road = BufferRoad::of(*segment, types);
            buffer.add(road, Counting::Count);
        }
        for road in std::mem::take(&mut buffer.roads) {
            buffer.add(road, Counting::Keep);
        }
        buffer.count();
        buffer.junctions();
        buffer.curves();
        buffer.joins();
        buffer
    }

    /// Its decals over `ground`, of the types of `types`, each type's texture at its place of
    /// `places`, by the type's place among the map's types, or why it has none: each type by its
    /// stacking, then in the order the game lists them (`drawRoads`), each of its pieces by their
    /// kind, then in its order (`loadRoadsInVertexAndIndexBuffers`). The pieces it builds, and
    /// those it drops, of no type and of a type of no place.
    pub(crate) fn draw(
        &self,
        types: &RoadsIni,
        ground: RoadGround<'_>,
        places: &[Result<TexturePlace, RoadDropCause>],
    ) -> DrawnRoads {
        debug_assert_eq!(places.len(), self.types.len(), "a place for each type");
        let mut decals = RoadDecals::default();
        let mut mesh = RoadMesh::default();
        let mut dropped = self.untyped.clone();
        for at in self.drawing_order() {
            let (road, place) = (self.types[at], &places[at]);
            let of_type = || self.roads.iter().filter(move |piece| piece.road == road);
            let place = match *place {
                Ok(place) => place,
                Err(cause) => {
                    let segments = of_type().filter(|piece| piece.kind == PieceKind::Segment);
                    dropped.push(DroppedRoads {
                        road: types[road].name.clone(),
                        cause,
                        count: segments.count(),
                    });
                    continue;
                }
            };
            for kind in PieceKind::ALL {
                for piece in of_type().filter(|piece| piece.kind == kind) {
                    mesh.float(&piece.piece(), ground);
                    decals.add(&mesh, place);
                }
            }
        }
        DrawnRoads {
            decals,
            pieces: self.pieces(),
            dropped,
        }
    }

    /// The places of the map's types in the order the game draws them: by their stacking, then
    /// in their order.
    fn drawing_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.types.len()).collect();
        order.sort_by_key(|&at| self.stacks.get(&self.types[at]).copied().unwrap_or(0));
        order
    }

    /// How many of each kind it holds.
    pub(crate) fn pieces(&self) -> RoadPieces {
        let of = |kind| self.roads.iter().filter(|road| road.kind == kind).count();
        RoadPieces {
            segments: of(PieceKind::Segment),
            curves: of(PieceKind::Curve),
            miters: self.miters,
            tees: of(PieceKind::Tee),
            four_ways: of(PieceKind::FourWay),
            ys: of(PieceKind::Y),
            hs: of(PieceKind::H) + of(PieceKind::HFlip),
            joins: of(PieceKind::AlphaJoin),
        }
    }

    /// Adds `road`, as `addMapObject` does: its points' corners set from its line; its ends
    /// counted, when `counting` counts; dropped when a piece of either direction is already
    /// between its points; else put after the first piece whose start meets an end of its that
    /// meets one other end of its type, or before the first whose end does, flipped to run on
    /// with it, then the chain followed on from it; else last.
    fn add(&mut self, mut road: BufferRoad, counting: Counting) {
        let [from, to] = [road.start.point, road.end.point];
        let normal = road.normal(to - from);
        for (point, at) in [(&mut road.start, from), (&mut road.end, to)] {
            point.top = at + normal;
            point.bottom = at - normal;
        }
        if counting == Counting::Count {
            self.count_ends(&mut road);
        }
        let mut flip = false;
        let mut before = false;
        let mut after = false;
        let mut at = self.roads.len();
        for (place, other) in self.roads.iter().enumerate() {
            let [start, end] = [other.start.point, other.end.point];
            if (start == from && end == to) || (start == to && end == from) {
                return;
            }
            if road.start.count == 1 {
                if start == from {
                    flip = true;
                    after = true;
                }
                if end == from {
                    flip = false;
                    before = true;
                }
            }
            if road.end.count == 1 {
                if start == to {
                    flip = false;
                    after = true;
                }
                if end == to {
                    flip = true;
                    before = true;
                }
            }
            if before || after {
                at = place;
                break;
            }
        }
        if after {
            at += 1;
        }
        if flip {
            road.flip();
        }
        self.roads.insert(at, road);
        if before {
            self.link_before(at);
        } else if after {
            self.link_after(at);
        }
    }

    /// Counts the ends of the pieces of `road`'s type that meet each of its ends, on both
    /// (`updateCounts`).
    fn count_ends(&mut self, road: &mut BufferRoad) {
        road.start.count = 0;
        road.end.count = 0;
        for other in self
            .roads
            .iter_mut()
            .filter(|other| other.road == road.road)
        {
            for mine in [&mut road.start, &mut road.end] {
                for theirs in [&mut other.start, &mut other.end] {
                    if theirs.point == mine.point {
                        theirs.count += 1;
                        mine.count += 1;
                    }
                }
            }
        }
    }

    /// Moves to `at`, before the road there, each piece after the chain from `at` on whose start
    /// meets the road's end, or whose end does, flipped, while the end it meets meets one other
    /// (`checkLinkBefore`).
    fn link_before(&mut self, at: usize) {
        if self.roads[at].end.count != 1 {
            return;
        }
        let mut to = self.roads[at].end.point;
        let mut chain_end = at + 1;
        while chain_end + 1 < self.roads.len()
            && self.roads[chain_end].start.point == self.roads[chain_end + 1].end.point
        {
            chain_end += 1;
        }
        let mut check = chain_end + 1;
        while check < self.roads.len() {
            let other = self.roads[check];
            if other.start.point == to || other.end.point == to {
                if other.start.point != to {
                    self.roads[check].flip();
                }
                self.move_road(check, at);
                to = self.roads[at].end.point;
                if self.roads[at].end.count != 1 {
                    return;
                }
                chain_end += 1;
            } else {
                check += 1;
            }
            if check <= chain_end {
                check = chain_end + 1;
            }
        }
    }

    /// Moves after the road at `at` each piece after it whose end meets its start, or whose
    /// start does, flipped, while the start it meets meets one other, the place it looks at
    /// kept as the game keeps it (`checkLinkAfter`).
    fn link_after(&mut self, mut at: usize) {
        if self.roads[at].start.count != 1 || at + 1 >= self.roads.len() {
            return;
        }
        let mut from = self.roads[at].start.point;
        let mut check = at + 1;
        while check < self.roads.len() && at + 1 < self.roads.len() {
            let other = self.roads[check];
            if other.end.point == from || other.start.point == from {
                if other.end.point != from {
                    self.roads[check].flip();
                }
                at += 1;
                self.move_road(check, at);
                from = self.roads[at].start.point;
                if self.roads[at].start.count != 1 {
                    return;
                }
            } else {
                check += 1;
            }
        }
    }

    /// Moves the piece at `from` to `to`, the pieces between shifting by one (`moveRoadSegTo`).
    fn move_road(&mut self, from: usize, to: usize) {
        let road = self.roads.remove(from);
        self.roads.insert(to, road);
    }

    /// Counts again, for each pair of pieces of a type, the ends of each that meet each end of
    /// the other (`updateCountsAndFlags`).
    fn count(&mut self) {
        for road in &mut self.roads {
            road.start.count = 0;
            road.end.count = 0;
        }
        for later in (1..self.roads.len()).rev() {
            let (before, rest) = self.roads.split_at_mut(later);
            let road = &mut rest[0];
            for other in before.iter_mut().filter(|other| other.road == road.road) {
                for theirs in [&mut other.start, &mut other.end] {
                    for mine in [&mut road.start, &mut road.end] {
                        if theirs.point == mine.point {
                            theirs.count += 1;
                            mine.count += 1;
                        }
                    }
                }
            }
        }
    }

    /// Inserts a curve or a miter where a piece's start meets the end of the piece after it, each
    /// end alone of its type there besides, and where a chain's first piece's start meets its
    /// last's end so (`insertCurveSegments`).
    fn curves(&mut self) {
        let count = self.roads.len();
        let mut chain_start = None;
        for at in 0..count {
            let next = at + 1;
            if next < count && self.roads[at].start.point == self.roads[next].end.point {
                if self.roads[next].end.count == 1 && self.roads[at].start.count == 1 {
                    self.curve(at, next);
                    chain_start.get_or_insert(at);
                }
            } else if let Some(first) = chain_start.take()
                && self.roads[at].start.point == self.roads[first].end.point
                && self.roads[first].end.count == 1
                && self.roads[at].start.count == 1
            {
                self.curve(at, first);
            }
        }
    }

    /// Joins the start of the piece at `out` to the end of the piece at `into`, which meet, by
    /// a curve, as `insertCurveSegmentAt` lays one: of its type's radius from the corner, both
    /// pieces cut back to where its arc meets each, in pieces of 30° each and a last of the rest;
    /// or by a miter, where its type is another, its turn is under `LEAST_CURVE` steps, its
    /// corner is angled, or the cut leaves less than `LEAST_LEFT` of either piece.
    fn curve(&mut self, out: usize, into: usize) {
        let [leaving, entering] = [self.roads[out], self.roads[into]];
        if leaving.road != entering.road {
            return;
        }
        let dir = |road: &BufferRoad| (road.end.point - road.start.point).normalize_or_zero();
        let [out_dir, in_dir] = [dir(&leaving), dir(&entering)];
        let cos = out_dir.dot(in_dir);
        let right = out_dir.perp_dot(in_dir) > 0.0;
        // The game names the four points so its first line leads into its second: the leaving
        // piece's on a right turn, else the entering's, reversed.
        let [first, second, third, fourth] = if right {
            [
                leaving.start.point,
                leaving.end.point,
                entering.start.point,
                entering.end.point,
            ]
        } else {
            [
                entering.end.point,
                entering.start.point,
                leaving.end.point,
                leaving.start.point,
            ]
        };
        let steps = cos.acos() / FRAC_PI_6;
        if steps < LEAST_CURVE || leaving.start.angled {
            self.miter(out, into);
            return;
        }
        let line_one = (second - first).normalize_or_zero();
        let line_two = (fourth - third).normalize_or_zero();
        let radius = leaving.radius() * leaving.width;
        let offset_one = plane::rotate(radius * line_one, -FRAC_PI_2);
        let offset_two = plane::rotate(radius * line_two, -FRAC_PI_2);
        let Some(center) = plane::intersection(
            [first + offset_one, second + offset_one],
            [third + offset_two, fourth + offset_two],
        ) else {
            return;
        };
        let on_two =
            plane::intersection([center, center - offset_two], [third, fourth]).unwrap_or(center);
        let on_one =
            plane::intersection([center, center - offset_one], [first, second]).unwrap_or(center);
        if line_two.dot(on_two - third) < LEAST_LEFT || line_one.dot(second - on_one) < LEAST_LEFT {
            self.miter(out, into);
            return;
        }
        let new_fourth = on_two;
        let new_first = on_one;
        let angle = -FRAC_PI_6;
        let mut at = new_fourth;
        let mut direction = third - at;
        let middle = direction.perp().normalize_or_zero() * radius + at;
        at = plane::rotate_about(at, middle, angle);
        direction = plane::rotate(direction, angle);
        self.roads.push(leaving.curve(at, at + direction));
        let mut step = 2.0;
        while step < steps {
            direction = plane::rotate(direction, angle);
            at = plane::rotate_about(at, middle, angle);
            self.roads.push(leaving.curve(at, at + direction));
            step += 1.0;
        }
        if steps > 1.0 {
            self.roads
                .push(leaving.curve(new_first, new_first + new_first - second));
        }
        let [leaving_start, entering_end] = if right {
            [new_first, new_fourth]
        } else {
            [new_fourth, new_first]
        };
        let road = &mut self.roads[out];
        road.start.point = leaving_start;
        let normal = road.normal(road.end.point - road.start.point);
        road.start.top = leaving_start + normal;
        road.start.bottom = leaving_start - normal;
        let road = &mut self.roads[into];
        road.end.point = entering_end;
        let normal = road.normal(road.end.point - road.start.point);
        road.end.top = entering_end + normal;
        road.end.bottom = entering_end - normal;
    }

    /// Meets the corners of the start of the piece at `out` and the end of the piece at `into` at
    /// the crossing of their sides, top with top and bottom with bottom, where they cross
    /// (`miter`).
    fn miter(&mut self, out: usize, into: usize) {
        self.miters += 1;
        let [leaving, entering] = [self.roads[out], self.roads[into]];
        let tops = [leaving, entering].map(|road| [road.start.top, road.end.top]);
        if let Some(corner) = plane::intersection(tops[0], tops[1]) {
            self.roads[into].end.top = corner;
            self.roads[out].start.top = corner;
        }
        let bottoms = [leaving, entering].map(|road| [road.start.bottom, road.end.bottom]);
        if let Some(corner) = plane::intersection(bottoms[0], bottoms[1]) {
            self.roads[into].end.bottom = corner;
            self.roads[out].start.bottom = corner;
        }
    }
}

mod buffer_road;
mod joins;
mod junctions;
mod plane;

#[cfg(test)]
mod tests;
