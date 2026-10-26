//! The buffer's alpha joins, as `insertCrossTypeJoins` inserts them where an end alone of its type
//! asks for one: the end cut square to the road of another type it meets, a piece that fades it
//! in, and the stacking of the two types, its own over the other.

use std::f32::consts::FRAC_PI_2;

use bevy_math::Vec2;

use crate::zero_hour::format::ini::roads_ini::RoadId;
use crate::zero_hour::road_buffer::RoadBuffer;
use crate::zero_hour::road_buffer::buffer_road::PieceKind;
use crate::zero_hour::road_buffer::plane;
use crate::zero_hour::road_buffer::plane::TurnSign;

/// How far a join's end may lie from a road of another type's line, in its widths
/// (`findCrossTypeJoinVector`).
const JOIN_REACH: f32 = 0.55;
/// How far a join reaches along its own road, in units, when no road of another type meets it.
const LONE_JOIN: f32 = 100.0;

impl RoadBuffer {
    /// Inserts an alpha join at each piece's end that meets none of its type and asks for one,
    /// the end first, then the start (`insertCrossTypeJoins`): along the road of another type its
    /// end meets, squared to it, its corners there cut to the join's line, its type stacked over
    /// the other's; or along its own road, `LONE_JOIN` long, where it meets none.
    pub(super) fn joins(&mut self) {
        for at in 0..self.roads.len() {
            let road = self.roads[at];
            let at_start = if road.end.count == 0 && road.end.join {
                false
            } else if road.start.count == 0 && road.start.join {
                true
            } else {
                continue;
            };
            let [from, to] = if at_start {
                [road.start.point, road.end.point]
            } else {
                [road.end.point, road.start.point]
            };
            let along = (to - from).normalize_or_zero();
            let met = self.join_along(from, along, road.road);
            let join = met.map_or(along * LONE_JOIN, |met| met.along);
            let normal = along.perp();
            let join_normal = join.perp();
            let half = normal * road.width * road.width_in_texture / 2.0;
            let join_line = [from, from + join_normal];
            let left = plane::intersection(join_line, [from + half, to + half]);
            let right = plane::intersection(join_line, [from - half, to - half]);
            let piece = &mut self.roads[at];
            // The end's corner on the road's left, and on its right, as the game's names fall.
            let top = if let Some(left) = left {
                if at_start {
                    piece.start.top = left;
                } else {
                    piece.end.bottom = left;
                }
                left
            } else {
                piece.start.top
            };
            let bottom = if let Some(right) = right {
                if at_start {
                    piece.start.bottom = right;
                } else {
                    piece.end.top = right;
                }
                right
            } else {
                piece.start.bottom
            };
            let across = (bottom - top).length() / (road.width * road.width_in_texture);
            if let Some(met) = met {
                self.stack(road.road, met.road);
            }
            let join_piece = road.junction(
                PieceKind::AlphaJoin,
                [from, from + join],
                0,
                road.width * across,
            );
            self.roads.push(join_piece);
        }
    }

    /// The direction of the join at `point` of a road of type `road` running along `along`: the
    /// first straight segment of another type whose box, half its width larger, holds the point,
    /// and whose line passes within `JOIN_REACH` of its widths of it, turned a right angle
    /// towards `along`, its own length long (`findCrossTypeJoinVector`).
    fn join_along(&self, point: Vec2, along: Vec2, road: RoadId) -> Option<JoinMet> {
        self.roads.iter().find_map(|other| {
            if other.road == road || other.kind != PieceKind::Segment {
                return None;
            }
            let [start, end] = [other.start.point, other.end.point];
            let half = other.width / 2.0;
            let low = start.min(end) - Vec2::splat(half);
            let high = start.max(end) + Vec2::splat(half);
            if point.cmplt(low).any() || point.cmpgt(high).any() {
                return None;
            }
            let line = end - start;
            let direction = line.normalize_or_zero();
            let reach = direction.dot(point - start);
            let closest = if reach <= 0.0 {
                start
            } else if reach >= line.length() {
                end
            } else {
                start + direction * reach
            };
            if closest.distance(point) >= other.width * JOIN_REACH {
                return None;
            }
            let angle = if plane::turn(line, along) == TurnSign::Left {
                FRAC_PI_2
            } else {
                -FRAC_PI_2
            };
            Some(JoinMet {
                along: plane::rotate(line, angle),
                road: other.road,
            })
        })
    }

    /// Stacks the type `top` over `bottom`, unless it is over it already: one above it, each type
    /// above it one higher (`adjustStacking`).
    fn stack(&mut self, top: RoadId, bottom: RoadId) {
        let [over, under] = [top, bottom].map(|id| self.stacks.get(&id).copied().unwrap_or(0));
        if over > under {
            return;
        }
        for level in self.stacks.values_mut() {
            if *level > under {
                *level += 1;
            }
        }
        self.stacks.insert(top, under + 1);
    }
}

/// A road of another type a join meets: the join's direction along it, and its type.
#[derive(Debug, Clone, Copy)]
struct JoinMet {
    along: Vec2,
    road: RoadId,
}
