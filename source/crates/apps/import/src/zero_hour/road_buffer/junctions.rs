//! The buffer's junctions, as `insertTeeIntersections` inserts them where three or four ends of a
//! type meet: the segments' ends moved to meet each junction's arms, and a piece of the junction.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use bevy_math::Vec2;

use crate::zero_hour::road_buffer::RoadBuffer;
use crate::zero_hour::road_buffer::buffer_road::{BufferPoint, PieceKind, TEE_WIDTH};
use crate::zero_hour::road_buffer::plane;
use crate::zero_hour::road_buffer::plane::TurnSign;

/// An end of a piece of the buffer: the piece's place, and which of its ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EndAt {
    road: usize,
    end: EndKind,
}

/// Which end of a piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndKind {
    Start,
    End,
}

/// An arm of a junction: its end at the junction's point, and its other end.
#[derive(Debug, Clone, Copy)]
struct Arm {
    center: EndAt,
    far: EndAt,
}

/// The junction's ends a mark sets: those met, besides the first, as the game marks them
/// (`count = -2`).
const MET: i32 = -2;
/// A three-way's and a four-way's mark on their start.
const THREE: i32 = -3;
const FOUR: i32 = -4;
/// The cosines past which two arms are a line for a Y, and a tee's arm slanted for an H.
const COS_30: f32 = 0.866;
const COS_45: f32 = 0.707;
const COS_60: f32 = 0.5;

impl RoadBuffer {
    /// Inserts a junction at each segment's end that meets two other ends of its type, a tee or a
    /// Y or an H, and at each that meets three, a four-way (`insertTeeIntersections`).
    pub(super) fn junctions(&mut self) {
        for at in 0..self.roads.len() {
            if self.roads[at].kind != PieceKind::Segment {
                continue;
            }
            for (count, end) in [
                (2, EndKind::Start),
                (2, EndKind::End),
                (3, EndKind::Start),
                (3, EndKind::End),
            ] {
                let point = self.end(EndAt { road: at, end });
                if point.count == count {
                    let point = point.point;
                    if count == 2 {
                        self.tee(point, at);
                    } else {
                        self.four_way(point, at);
                    }
                }
            }
        }
    }

    fn end(&self, at: EndAt) -> &BufferPoint {
        let road = &self.roads[at.road];
        match at.end {
            EndKind::Start => &road.start,
            EndKind::End => &road.end,
        }
    }

    fn end_mut(&mut self, at: EndAt) -> &mut BufferPoint {
        let road = &mut self.roads[at.road];
        match at.end {
            EndKind::Start => &mut road.start,
            EndKind::End => &mut road.end,
        }
    }

    /// The arms at `point`: the end of the piece at `first` there, then each end there of a piece
    /// after it, of any type, each marked as met.
    fn arms(&mut self, point: Vec2, first: usize) -> Vec<Arm> {
        let own = if self.roads[first].start.point == point {
            EndKind::Start
        } else {
            EndKind::End
        };
        let other = |end| match end {
            EndKind::Start => EndKind::End,
            EndKind::End => EndKind::Start,
        };
        let mut arms = vec![Arm {
            center: EndAt {
                road: first,
                end: own,
            },
            far: EndAt {
                road: first,
                end: other(own),
            },
        }];
        for road in first + 1..self.roads.len() {
            for end in [EndKind::Start, EndKind::End] {
                let at = EndAt { road, end };
                if self.end(at).point == point {
                    self.end_mut(at).count = MET;
                    arms.push(Arm {
                        center: at,
                        far: EndAt {
                            road,
                            end: other(end),
                        },
                    });
                }
            }
        }
        arms
    }

    /// The direction from `point` to each arm's far end, of unit length, or zero.
    fn directions<const N: usize>(&self, point: Vec2, arms: [Arm; N]) -> [Vec2; N] {
        arms.map(|arm| (self.end(arm.far).point - point).normalize_or_zero())
    }

    /// Inserts a Y at `point`, where the end of the piece at `first` meets two others, or else a
    /// tee, or an H where its arm is slanted (`insertTee`): of the arms the first meets and the
    /// last after it, the two most opposite are the tee's straight road, and the third its arm.
    fn tee(&mut self, point: Vec2, first: usize) {
        let arms = self.arms(point, first);
        if arms.len() < 3 {
            return;
        }
        let arms = [arms[0], arms[1], arms[arms.len() - 1]];
        if self.y(point, first, arms) {
            return;
        }
        let [v1, v2, v3] = self.directions(point, arms);
        let [d12, d13, d32] = [v1.dot(v2), v1.dot(v3), v3.dot(v2)];
        // The pair of the least dot product is the straight road: its arms, in the order the game
        // passes them, then the tee's arm.
        let [straight_from, straight_to, arm] = if d12 < d13 {
            if d12 < d32 { [0, 1, 2] } else { [2, 1, 0] }
        } else if d13 < d32 {
            [0, 2, 1]
        } else {
            [2, 1, 0]
        };
        let v = [v1, v2, v3];
        let up = (v[straight_to] - v[straight_from]).normalize_or_zero();
        let decider = v[arm];
        let width = self.roads[first].width;
        let width_in_texture = self.roads[first].width_in_texture;
        let mirror = up.perp_dot(decider) < 0.0;
        let angle = if mirror { -FRAC_PI_2 } else { FRAC_PI_2 };
        let up = up.normalize_or_zero() * 0.5 * width;
        let tee = plane::rotate(up, angle);
        let [one, two, three] = [arms[straight_from], arms[straight_to], arms[arm]];
        let kind = if up.normalize_or_zero().dot(decider).abs() > COS_60 {
            let flip = plane::turn(tee, decider) == TurnSign::Left;
            self.offset_h(
                Junction {
                    arms: [one, two, three],
                    point,
                    up,
                    tee,
                    width_in_texture,
                },
                HShape { flip, mirror },
            );
            if flip { PieceKind::HFlip } else { PieceKind::H }
        } else {
            self.offset_three(Junction {
                arms: [one, two, three],
                point,
                up,
                tee,
                width_in_texture,
            });
            PieceKind::Tee
        };
        for arm in arms {
            self.end_mut(arm.center).count = 0;
        }
        let junction =
            self.roads[first].junction(kind, [point, point + tee], THREE, width_in_texture);
        self.roads.push(junction);
    }

    /// Inserts a Y at `point` when its three arms make one (`insertY`): no two near a line, and
    /// two legs, each on its own side of the third, the stem, both ahead of its normal; of the
    /// stems that fit, the one whose legs' dot products with it lie nearest 135°. Whether it did.
    fn y(&mut self, point: Vec2, first: usize, arms: [Arm; 3]) -> bool {
        let [v1, v2, v3] = self.directions(point, arms);
        let [d12, d13, d32] = [v1.dot(v2), v1.dot(v3), v3.dot(v2)];
        if [d12, d13, d32].iter().any(|&dot| dot < -COS_30) {
            return false;
        }
        // The stem's score, by the stem's arm, of 2 when it is none.
        let score = |stem: Vec2, legs: [Vec2; 2], dots: [f32; 2]| {
            let [s1, s2] = legs.map(|leg| plane::turn(stem, leg));
            let normal = stem.perp();
            let ahead = legs
                .iter()
                .all(|&leg| plane::turn(normal, leg) == TurnSign::Left);
            (matches!(
                (s1, s2),
                (TurnSign::Left, TurnSign::Right) | (TurnSign::Right, TurnSign::Left)
            ) && ahead)
                .then(|| (dots[0] + COS_45).abs() + (dots[1] + COS_45).abs())
        };
        let score32 = score(v1, [v2, v3], [d12, d13]);
        let score12 = score(v3, [v1, v2], [d13, d32]);
        let score13 = score(v2, [v1, v3], [d12, d32]);
        let [s12, s13, s32] = [score12, score13, score32].map(|score| score.unwrap_or(2.0));
        let chosen = if s12 < s13 {
            if s12 < s32 {
                score12.map(|_| 12)
            } else {
                score32.map(|_| 32)
            }
        } else if s13 < s32 {
            score13.map(|_| 13)
        } else {
            score32.map(|_| 32)
        };
        let Some(chosen) = chosen else {
            return false;
        };
        let [c1, c2, c3] = arms;
        // The stem's arm and the legs as `offsetY` takes them, the right leg first.
        let (stem, [left_leg, right_leg]) = match chosen {
            12 if plane::turn(v3, v1) == TurnSign::Right => (v3, [c1, c2]),
            12 => (v3, [c2, c1]),
            13 if plane::turn(v2, v1) == TurnSign::Right => (v2, [c1, c3]),
            13 => (v2, [c3, c1]),
            _ if plane::turn(v1, v3) == TurnSign::Right => (v1, [c3, c2]),
            _ => (v1, [c2, c3]),
        };
        let stem_arm = match chosen {
            12 => c3,
            13 => c2,
            _ => c1,
        };
        let width = self.roads[first].width;
        let width_in_texture = self.roads[first].width_in_texture;
        let up = stem.normalize_or_zero() * 0.5 * width;
        let tee = plane::rotate(up, -FRAC_PI_2);
        self.offset_y([left_leg, right_leg, stem_arm], point, up, width_in_texture);
        for arm in arms {
            self.end_mut(arm.center).count = 0;
        }
        let junction =
            self.roads[first].junction(PieceKind::Y, [point, point + tee], THREE, width_in_texture);
        self.roads.push(junction);
        true
    }

    /// Inserts a four-way at `point`, where the end of the piece at `first` meets three others
    /// (`insert4Way`): its two arms most opposite aligned with its straight road, the other two
    /// across it.
    fn four_way(&mut self, point: Vec2, first: usize) {
        let arms = self.arms(point, first);
        if arms.len() < 4 {
            return;
        }
        let arms = [arms[0], arms[1], arms[2], arms[arms.len() - 1]];
        let v = self.directions(point, arms);
        let pairs = [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]];
        let mut chosen = pairs[0];
        let mut least = v[0].dot(v[1]);
        for pair in &pairs[1..] {
            let dot = v[pair[0]].dot(v[pair[1]]);
            if dot < least {
                least = dot;
                chosen = *pair;
            }
        }
        let width = self.roads[first].width;
        let width_in_texture = self.roads[first].width_in_texture;
        let align = (v[chosen[1]] - v[chosen[0]]).normalize_or_zero() * 0.5 * width;
        // The other two arms, the first of them crossing the road's line as the game passes it.
        let [across, last] = match chosen {
            [0, 1] => [2, 3],
            [0, 2] => [1, 3],
            [0, 3] => [2, 1],
            [1, 2] => [0, 3],
            [1, 3] => [0, 2],
            _ => [0, 1],
        };
        self.offset_four(
            [arms[chosen[0]], arms[chosen[1]], arms[across], arms[last]],
            point,
            align,
            width_in_texture,
        );
        // A four-way is symmetric, so it runs east.
        let align = if align.x < 0.0 { -align } else { align };
        let junction =
            self.roads[first].junction(PieceKind::FourWay, [point, point + align], FOUR, TEE_WIDTH);
        self.roads.push(junction);
    }

    /// Moves a tee's ends to its arms (`offset3Way`): the straight road's back and on by `up`,
    /// the arm's to `tee`, each end's corners across its own line, on the side its top was.
    fn offset_three(&mut self, junction: Junction) {
        let Junction {
            arms: [one, two, three],
            point,
            up,
            tee,
            width_in_texture,
        } = junction;
        self.end_mut(one.center).point = point - up;
        self.end_mut(two.center).point = point + up;
        self.end_mut(three.center).point = point + tee;
        let right = Self::right_of(up, tee) * width_in_texture;
        for arm in [one, two] {
            self.square(arm.center, up, right, Side::Left);
        }
        self.square(three.center, right, up * width_in_texture, Side::Right);
    }

    /// Moves a slanted tee's ends to its arms (`offsetH`): the straight road's back and on by
    /// `up`, the farther 2.05 of it and the nearer 0.46, as `shape` places the arm; and
    /// the arm's on 2.10 along `tee` turned 45° towards it.
    fn offset_h(&mut self, junction: Junction, shape: HShape) {
        let HShape { flip, mirror } = shape;
        let Junction {
            arms: [one, two, three],
            point,
            up,
            tee,
            width_in_texture,
        } = junction;
        let [back, on] = if flip == mirror {
            [0.46, 2.05]
        } else {
            [2.05, 0.46]
        };
        self.end_mut(one.center).point = point - up * back;
        self.end_mut(two.center).point = point + up * on;
        let right = Self::right_of(up, tee) * width_in_texture;
        for arm in [one, two] {
            self.square(arm.center, up, right, Side::Left);
        }
        let turn = if flip { FRAC_PI_4 } else { -FRAC_PI_4 };
        let arm = plane::rotate(tee, turn);
        let center = self.end_mut(three.center);
        center.point += arm * 2.10;
        Self::across_arm(center, point, arm, width_in_texture);
    }

    /// Moves a Y's ends to its arms (`offsetY`): the stem's on by 0.55 of `up`, its corners with
    /// it, and each leg's out by 1.1 along `up` turned 135° to its side.
    fn offset_y(
        &mut self,
        [left, right, stem]: [Arm; 3],
        point: Vec2,
        up: Vec2,
        width_in_texture: f32,
    ) {
        let stem = self.end_mut(stem.center);
        stem.point += up * 0.55;
        stem.top += up * 0.55;
        stem.bottom += up * 0.55;
        for (leg, turn) in [(right, 3.0 * PI / 4.0), (left, -3.0 * PI / 4.0)] {
            let arm = plane::rotate(up, turn);
            let center = self.end_mut(leg.center);
            center.point += arm * 1.1;
            Self::across_arm(center, point, arm, width_in_texture);
        }
    }

    /// Moves a four-way's ends to its arms (`offset4Way`): the straight road's back and on by
    /// `align`, the first other's to `align` turned a right angle towards its far end and the
    /// last's opposite, each end's corners across its own line, on the side its top was.
    fn offset_four(
        &mut self,
        [one, two, three, four]: [Arm; 4],
        point: Vec2,
        align: Vec2,
        width_in_texture: f32,
    ) {
        self.end_mut(one.center).point = point - align;
        self.end_mut(two.center).point = point + align;
        let toward = self.end(three.far).point - point;
        let angle = if align.perp_dot(toward) < 0.0 {
            -FRAC_PI_2
        } else {
            FRAC_PI_2
        };
        let tee = plane::rotate(align, angle);
        self.end_mut(three.center).point = point + tee;
        self.end_mut(four.center).point = point - tee;
        let across = plane::rotate(align, FRAC_PI_2) * width_in_texture;
        for arm in [one, two] {
            self.square(arm.center, align, across, Side::Left);
        }
        for arm in [three, four] {
            self.square(arm.center, across, align * width_in_texture, Side::Right);
        }
        for arm in [one, two, three, four] {
            self.end_mut(arm.center).count = 0;
        }
    }

    /// `tee`, or its opposite, whichever lies to the left of `up`.
    fn right_of(up: Vec2, tee: Vec2) -> Vec2 {
        if up.perp_dot(tee) < 0.0 { -tee } else { tee }
    }

    /// Sets the corners of the end at `at` to its point plus and less `offset`, its top plus when
    /// its top lay on `side` of `line`, else less.
    fn square(&mut self, at: EndAt, line: Vec2, offset: Vec2, side: Side) {
        let end = self.end_mut(at);
        let turn = line.perp_dot(end.top - end.point);
        let plus = match side {
            Side::Left => turn > 0.0,
            Side::Right => turn < 0.0,
        };
        let [top, bottom] = if plus {
            [end.point + offset, end.point - offset]
        } else {
            [end.point - offset, end.point + offset]
        };
        end.top = top;
        end.bottom = bottom;
    }

    /// Sets an arm's end's corners across `arm`, its top on the side of `arm` its top lay from
    /// the junction's `point`.
    fn across_arm(end: &mut BufferPoint, point: Vec2, arm: Vec2, width_in_texture: f32) {
        let normal = arm.perp() * width_in_texture;
        let [top, bottom] = if plane::turn(arm, end.top - point) == TurnSign::Left {
            [end.point + normal, end.point - normal]
        } else {
            [end.point - normal, end.point + normal]
        };
        end.top = top;
        end.bottom = bottom;
    }
}

/// A three-way's arms, the straight road's back and on then the arm; its point, its offset along
/// the straight road and the arm's, and the width its type's texture draws.
#[derive(Debug, Clone, Copy)]
struct Junction {
    arms: [Arm; 3],
    point: Vec2,
    up: Vec2,
    tee: Vec2,
    width_in_texture: f32,
}

/// How an H lies: whether its arm turns to the left of its tee, `THREE_WAY_H_FLIP`, and whether
/// the tee turned right of the straight road, to meet the arm.
#[derive(Debug, Clone, Copy)]
struct HShape {
    flip: bool,
    mirror: bool,
}

/// Which side of a line an end's top lies on for its top to take the offset: a straight road's
/// ends' left, an arm's right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}
