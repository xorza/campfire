use bevy_math::Vec2;
use campfire_common::EnumAll;

use crate::zero_hour::format::ini::roads_ini::{RoadId, RoadsIni};
use crate::zero_hour::map_roads::{RoadEnd, RoadSegment};
use crate::zero_hour::road_mesh::{PieceCorners, RoadPiece};

/// A piece of the buffer: its type, its kind, its two points, its type's width and the share of
/// it its texture draws, and whether a curve at its start is tight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BufferRoad {
    pub(super) road: RoadId,
    pub(super) kind: PieceKind,
    pub(super) start: BufferPoint,
    pub(super) end: BufferPoint,
    pub(super) width: f32,
    pub(super) width_in_texture: f32,
    pub(super) tight: bool,
}

/// A piece's kind, in the order the game draws a type's pieces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll)]
pub(super) enum PieceKind {
    Segment,
    Curve,
    Tee,
    FourWay,
    /// `THREE_WAY_Y`: three ends, none near another's straight line.
    Y,
    /// `THREE_WAY_H`: a tee whose arm meets its straight road slanted, and its mirror.
    H,
    HFlip,
    AlphaJoin,
}

/// A point of a piece: where it is, its corners on the road's left and right as the game keeps
/// them, its top and bottom, how many other ends of its type share it, and its end's flags.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BufferPoint {
    pub(super) point: Vec2,
    pub(super) top: Vec2,
    pub(super) bottom: Vec2,
    pub(super) count: i32,
    pub(super) angled: bool,
    pub(super) join: bool,
}

/// A segment's place in its type's texture, in shares of its side: its `u` from 0 and its `v`
/// from 85 512ths, across its middle (`preloadRoadSegment`).
pub(super) const SEGMENT_UV: Vec2 = Vec2::new(0.0, 85.0 / 512.0);
/// A curve's place, of a curve of 1.5 road widths and of a tight one of 0.5 (`loadCurve`).
pub(super) const CURVE_UV: Vec2 = Vec2::new(4.0 / 512.0, 255.0 / 512.0);
const TIGHT_CURVE_UV: Vec2 = Vec2::new(4.0 / 512.0, 425.0 / 512.0);
/// A tee's and a four-way's place, a Y's, and an H's (`loadTee`, `loadY`, `loadH`).
const TEE_UV: Vec2 = Vec2::new(425.0 / 512.0, 255.0 / 512.0);
const FOUR_WAY_UV: Vec2 = Vec2::new(425.0 / 512.0, 425.0 / 512.0);
const Y_UV: Vec2 = Vec2::new(255.0 / 512.0, 226.0 / 512.0);
const H_UV: Vec2 = Vec2::new(202.0 / 512.0, 364.0 / 512.0);
/// An alpha join's place, its length in 128ths of a road width, and its width past its texture's
/// in 128ths (`loadAlphaJoin`).
const JOIN_UV: Vec2 = Vec2::new(106.0 / 512.0, 425.0 / 512.0);
const JOIN_LENGTH: f32 = 48.0 / 128.0;
const JOIN_MARGIN: f32 = 8.0 / 128.0;
/// A junction's width in road widths (`TEE_WIDTH_ADJUSTMENT`), and the shortest line a
/// junction's direction is taken of, along each axis (`MIN_ROAD_SEGMENT`).
pub(super) const TEE_WIDTH: f32 = 1.03;
const SHORTEST: f32 = 0.25;
/// A curve's radius in road widths (`CORNER_RADIUS`, `TIGHT_CORNER_RADIUS`).
const CURVE_RADIUS: f32 = 1.5;
const TIGHT_CURVE_RADIUS: f32 = 0.5;

impl BufferRoad {
    /// The segment `segment` as the buffer takes it, of its type's width.
    pub(super) fn of(segment: RoadSegment, types: &RoadsIni) -> Self {
        let road = &types[segment.road];
        let point = |end: RoadEnd| BufferPoint {
            point: end.point,
            top: end.point,
            bottom: end.point,
            count: 0,
            angled: end.angled,
            join: end.join,
        };
        Self {
            road: segment.road,
            kind: PieceKind::Segment,
            start: point(segment.start),
            end: point(segment.end),
            width: road.width,
            width_in_texture: road.width_in_texture,
            tight: segment.tight,
        }
    }

    /// A curve of its type and its radius from `start` to `end`.
    pub(super) fn curve(&self, start: Vec2, end: Vec2) -> Self {
        let point = |at: Vec2| BufferPoint {
            point: at,
            top: at,
            bottom: at,
            count: 0,
            angled: false,
            join: false,
        };
        Self {
            kind: PieceKind::Curve,
            start: point(start),
            end: point(end),
            ..*self
        }
    }

    /// A junction of `kind` of its type, from `start` to `end`, the point it is at and its
    /// direction, its start counted `count`, as the game marks a junction's, and its width in its
    /// texture `width_in_texture`.
    pub(super) fn junction(
        &self,
        kind: PieceKind,
        [start, end]: [Vec2; 2],
        count: i32,
        width_in_texture: f32,
    ) -> Self {
        let mut junction = Self {
            kind,
            width_in_texture,
            ..self.curve(start, end)
        };
        junction.start.count = count;
        junction
    }

    /// Swaps its start and its end, each whole (`flipTheRoad`).
    pub(super) const fn flip(&mut self) {
        std::mem::swap(&mut self.start, &mut self.end);
    }

    /// Its curves' radius, in road widths.
    pub(super) const fn radius(&self) -> f32 {
        if self.tight {
            TIGHT_CURVE_RADIUS
        } else {
            CURVE_RADIUS
        }
    }

    /// The normal to the left of `along`, half the width its texture draws long.
    pub(super) fn normal(&self, along: Vec2) -> Vec2 {
        along.perp().normalize_or_zero() * (self.width * self.width_in_texture / 2.0)
    }

    /// The piece the game floats of it: a segment's quadrilateral of its points' corners, its
    /// texture from `SEGMENT_UV` at its start (`preloadRoadSegment`); a curve's, a road width
    /// along from its start and as wide as its texture draws, its corners pulled in as the game
    /// tweaks them, its texture from its curve's place (`loadCurve`); a junction's, about its
    /// point along its direction, from its own place (`loadTee`, `loadY`, `loadH`); an alpha
    /// join's, from its end along the road it meets, its `v` scaled by the width it cuts
    /// (`loadAlphaJoin`).
    pub(super) fn piece(&self) -> RoadPiece {
        let [start, end] = [self.start.point, self.end.point];
        let scale = self.width;
        match self.kind {
            PieceKind::Segment => {
                let along = end - start;
                RoadPiece {
                    corners: PieceCorners {
                        start_right: self.start.bottom,
                        start_left: self.start.top,
                        end_right: self.end.bottom,
                        end_left: self.end.top,
                    },
                    origin: start,
                    uv: SEGMENT_UV,
                    along,
                    normal: self.normal(along),
                    scale: Vec2::splat(scale),
                }
            }
            PieceKind::Curve => {
                let along = (end - start).normalize_or_zero() * scale;
                let half = (self.width * self.width_in_texture / 2.0).abs();
                let normal = (end - start).perp().normalize_or_zero() * half;
                let start_right = start - normal;
                let [reach, pull, side] = if self.tight {
                    [0.5, 0.1, 0.2]
                } else {
                    [1.0, 0.2, 0.4]
                };
                let mut end_right = start_right + along * reach;
                let mut end_left = end_right + 2.0 * normal;
                let mut start_left = start_right + 2.0 * normal;
                let mut start_right = start_right;
                end_right += along * 0.1;
                end_right += normal * side;
                start_right -= normal * pull;
                start_right -= along * 0.02;
                start_left -= along * 0.02;
                end_left -= along * 0.4;
                end_left += normal * side;
                RoadPiece {
                    corners: PieceCorners {
                        start_right,
                        start_left,
                        end_right,
                        end_left,
                    },
                    origin: start,
                    uv: if self.tight { TIGHT_CURVE_UV } else { CURVE_UV },
                    along,
                    normal,
                    scale: Vec2::splat(scale),
                }
            }
            PieceKind::Tee | PieceKind::FourWay => {
                let tee = scale * TEE_WIDTH / 2.0;
                let left = self.width_in_texture * scale / 2.0;
                let uv = if self.kind == PieceKind::FourWay {
                    FOUR_WAY_UV
                } else {
                    TEE_UV
                };
                Self::float_section(start, end - start, [tee, left, tee], uv, scale)
            }
            PieceKind::Y => {
                let [along, normal] = Self::axes(end - start).map(|axis| axis * scale);
                let along = along * 1.59;
                let start_left = start + normal * 0.29 - along * 0.5;
                let start_right = start_left - normal * 1.08;
                RoadPiece {
                    corners: PieceCorners {
                        start_right,
                        start_left,
                        end_right: start_right + along,
                        end_left: start_left + along,
                    },
                    origin: start,
                    uv: Y_UV,
                    along,
                    normal,
                    scale: Vec2::splat(scale),
                }
            }
            PieceKind::AlphaJoin => {
                let [along, normal] = Self::axes(end - start);
                let across = self.width_in_texture;
                let along = along * (scale * JOIN_LENGTH);
                let normal = normal * (across * (1.0 + JOIN_MARGIN));
                let start_left = start + normal * 0.5 - along * 0.65;
                let start_right = start_left - normal;
                RoadPiece {
                    corners: PieceCorners {
                        start_right,
                        start_left,
                        end_right: start_right + along,
                        end_left: start_left + along,
                    },
                    origin: start,
                    uv: JOIN_UV,
                    along,
                    normal,
                    scale: Vec2::new(scale, across),
                }
            }
            PieceKind::H | PieceKind::HFlip => {
                let flip = self.kind == PieceKind::HFlip;
                let [along, normal] = Self::axes(end - start).map(|axis| axis * scale);
                let normal = normal * 1.35;
                let back = if flip { 0.20 } else { 0.8 };
                let start_right = start - normal * back - along * self.width_in_texture / 2.0;
                let width = along * self.width_in_texture / 2.0 + along * 1.2;
                let end_right = start_right + width;
                RoadPiece {
                    corners: PieceCorners {
                        start_right,
                        start_left: start_right + normal,
                        end_right,
                        end_left: end_right + normal,
                    },
                    origin: start,
                    uv: H_UV,
                    along,
                    normal: if flip { -normal } else { normal },
                    scale: Vec2::splat(scale),
                }
            }
        }
    }

    /// The unit vector of `along` and its normal, to its left; along `x` and `y` for a line
    /// shorter than `SHORTEST` on both axes.
    fn axes(along: Vec2) -> [Vec2; 2] {
        if along.x.abs() < SHORTEST && along.y.abs() < SHORTEST {
            [Vec2::X, Vec2::Y]
        } else {
            [along.normalize_or_zero(), along.perp().normalize_or_zero()]
        }
    }

    /// The piece of a section from `origin` along `along`, `[half, left, right]`: `half` to each
    /// side, `left` back from the origin and `right` on from it (`loadFloatSection`).
    fn float_section(
        origin: Vec2,
        along: Vec2,
        [half, left, right]: [f32; 3],
        uv: Vec2,
        scale: f32,
    ) -> RoadPiece {
        let normal = along.perp().normalize_or_zero() * half.abs();
        let unit = along.normalize_or_zero();
        let back = unit * left;
        let along = unit * right + back;
        let start_right = origin - back - normal;
        let end_right = start_right + along;
        RoadPiece {
            corners: PieceCorners {
                start_right,
                start_left: start_right + 2.0 * normal,
                end_right,
                end_left: end_right + 2.0 * normal,
            },
            origin,
            uv,
            along,
            normal,
            scale: Vec2::splat(scale),
        }
    }
}
