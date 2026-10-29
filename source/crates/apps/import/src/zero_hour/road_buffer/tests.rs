use bevy_math::Vec2;

use super::*;
use crate::zero_hour::map_roads::RoadEnd;
use crate::zero_hour::road_buffer::buffer_road::{CURVE_UV, SEGMENT_UV};

/// A segment of the sample gravel road, 35 wide and 0.9 of its texture across, 15.75 to each
/// side, from `start` to `end`, its start's corner `angled`.
fn segment(start: [f32; 2], end: [f32; 2], angled: bool) -> RoadSegment {
    let end_of = |point: [f32; 2], angled: bool| RoadEnd {
        point: Vec2::from(point),
        angled,
        join: false,
    };
    RoadSegment {
        road: RoadId(0),
        width: 35.0,
        width_in_texture: 0.9,
        start: end_of(start, angled),
        end: end_of(end, false),
        tight: false,
    }
}

/// Whether `found` lies within `1e-3` of `expected`: the curve's points are sines and roots
/// that an `f32` rounds.
fn near(found: Vec2, expected: [f32; 2]) -> bool {
    found.abs_diff_eq(Vec2::from(expected), 1e-3)
}

#[test]
fn two_segments_that_meet_chain_back_along_the_list_and_turn_by_a_curve() {
    // A from (0, 0) east to (100, 0), then B on north-east to (200, 100): B is put before A, as
    // a chain runs back along the list, and B's start and A's end meet alone.
    let forward = [
        segment([0.0, 0.0], [100.0, 0.0], false),
        segment([100.0, 0.0], [200.0, 100.0], false),
    ];
    let buffer = RoadBuffer::chained(&forward);
    // B given the other way meets A's end with its own, so it is flipped to run on: the same.
    let reversed = [forward[0], segment([200.0, 100.0], [100.0, 0.0], false)];
    assert_eq!(RoadBuffer::chained(&reversed).roads, buffer.roads);
    // A segment given again, either way, is dropped.
    let twice = [
        forward[0],
        forward[1],
        segment([100.0, 0.0], [0.0, 0.0], false),
    ];
    assert_eq!(RoadBuffer::chained(&twice).roads, buffer.roads);

    // The turn is 45°, 1.5 steps of 30°, to the left, so the game's lines run from A's end back
    // to its start and from B's end back to its start. A curve 1.5 · 35 = 52.5 from both: its
    // centre on y = 52.5 and on B's line moved 52.5 to its left, y = x − 100 + 52.5 · √2, so at
    // x = 152.5 − 74.246 = 78.254. A is cut back to the centre's foot on it, (78.254, 0), and B
    // to its foot on B, (115.377, 15.377).
    let roads = &buffer.roads;
    assert_eq!(roads.len(), 4);
    let [b, a] = [roads[0], roads[1]];
    assert_eq!(
        [a.start.point, b.end.point],
        [Vec2::ZERO, Vec2::new(200.0, 100.0)]
    );
    assert!(near(a.end.point, [78.253_79, 0.0]));
    assert!(near(b.start.point, [115.376_89, 15.376_894]));
    // The first curve from B's cut turned 30° about the centre, to (91.842, 1.789), along B's
    // line turned 30°, at 15°; the last, of the rest of the turn, from A's cut on along A. Both
    // a sharp turn's two steps fit, so each end stays a segment's, and the corner no miter.
    let [first, last] = [roads[2], roads[3]];
    assert_eq!([first.kind, last.kind], [PieceKind::Curve; 2]);
    assert!(near(first.start.point, [91.841_79, 1.788_894]));
    let direction = |road: BufferRoad| (road.end.point - road.start.point).normalize();
    let fifteen = 15.0_f32.to_radians();
    assert!(near(
        direction(first),
        [libm::cosf(fifteen), libm::sinf(fifteen)]
    ));
    assert!(near(last.start.point, [78.253_79, 0.0]));
    assert!(near(direction(last), [1.0, 0.0]));
    assert_eq!(
        buffer.pieces(),
        RoadPieces {
            segments: 2,
            curves: 2,
            ..RoadPieces::default()
        }
    );
    // A curve's piece: a road width along from its start, from its place in the texture.
    let piece = first.piece();
    assert_eq!(piece.uv, CURVE_UV);
    assert!((piece.along.length() - 35.0).abs() < 1e-4);
    assert!((piece.normal.length() - 15.75).abs() < 1e-4);
}

#[test]
fn a_segment_of_no_type_chains_to_the_type_it_is_given_at_its_own_width() {
    // A as in the first test, then B of a template no type names, which the map's roads give
    // the gravel road, 8 wide and all its texture across: of one type, they chain and turn by
    // curves, of B's width, as B leaves the corner. The curve 1.5 · 8 = 12 from both lines:
    // its centre on y = 12 and on y = x − 100 + 12 · √2, at x = 112 − 16.971 = 95.029, where A
    // is cut back.
    let untyped = RoadSegment {
        width: 8.0,
        width_in_texture: 1.0,
        ..segment([100.0, 0.0], [200.0, 100.0], false)
    };
    let buffer = RoadBuffer::chained(&[segment([0.0, 0.0], [100.0, 0.0], false), untyped]);
    let roads = &buffer.roads;
    let kinds: Vec<PieceKind> = roads.iter().map(|road| road.kind).collect();
    let [segment, curve] = [PieceKind::Segment, PieceKind::Curve];
    assert_eq!(kinds, [segment, segment, curve, curve]);
    let widths: Vec<f32> = roads.iter().map(|road| road.width).collect();
    assert_eq!(widths, [8.0, 35.0, 8.0, 8.0]);
    assert!(roads.iter().all(|road| road.road == RoadId(0)));
    assert!(near(
        roads[1].end.point,
        [112.0 - 12.0 * 2.0_f32.sqrt(), 0.0]
    ));
    // B's piece spans its 8 across: 4 to each side.
    assert!((roads[0].piece().normal.length() - 4.0).abs() < 1e-4);
}

#[test]
fn an_angled_corner_meets_its_sides_at_their_crossings() {
    // B's start, at the corner, is angled: a miter. A's top side is y = 15.75 and B's runs from
    // (100, 0) moved 15.75 to its left, (88.863, 11.137), at 45°, so they cross at x = 88.863 +
    // 4.613 = 93.476; their bottoms, at y = −15.75 and from (111.137, −11.137), at 106.524.
    let segments = [
        segment([0.0, 0.0], [100.0, 0.0], false),
        segment([100.0, 0.0], [200.0, 100.0], true),
    ];
    let buffer = RoadBuffer::chained(&segments);
    let [b, a] = [buffer.roads[0], buffer.roads[1]];
    assert_eq!(buffer.roads.len(), 2);
    assert!(near(a.end.top, [93.476_14, 15.75]));
    assert!(near(b.start.top, [93.476_14, 15.75]));
    assert!(near(a.end.bottom, [106.523_86, -15.75]));
    assert!(near(b.start.bottom, [106.523_86, -15.75]));
    // The points stay; the far corners keep their square ends.
    assert_eq!([a.end.point, b.start.point], [Vec2::new(100.0, 0.0); 2]);
    assert_eq!(a.start.top, Vec2::new(0.0, 15.75));
    assert_eq!(buffer.pieces().miters, 1);
    // A segment's piece is its points' corners: the miter's at its end.
    let piece = a.piece();
    assert_eq!(piece.corners.end_left, a.end.top);
    assert_eq!(piece.corners.start_right, Vec2::new(0.0, -15.75));
    assert_eq!(piece.uv, SEGMENT_UV);
}

/// The buffer of gravel roads from the origin to each of `ends`, a road's half width 17.5; its
/// junctions, and the distances of its segments' ends at the origin from it, the least first.
fn junction(ends: &[[f32; 2]]) -> (Vec<BufferRoad>, Vec<f32>) {
    let segments: Vec<RoadSegment> = ends
        .iter()
        .map(|&end| segment([0.0, 0.0], end, false))
        .collect();
    let buffer = RoadBuffer::chained(&segments);
    let junctions = buffer
        .roads
        .iter()
        .filter(|road| road.kind != PieceKind::Segment)
        .copied()
        .collect();
    let mut near: Vec<f32> = buffer
        .roads
        .iter()
        .filter(|road| road.kind == PieceKind::Segment)
        .flat_map(|road| [road.start.point, road.end.point])
        .map(Vec2::length)
        .filter(|&length| length < 50.0)
        .collect();
    near.sort_by(f32::total_cmp);
    (junctions, near)
}

/// Whether each of `found` lies within `1e-3` of `expected`'s.
fn all_near(found: &[f32], expected: &[f32]) -> bool {
    found.len() == expected.len()
        && found
            .iter()
            .zip(expected)
            .all(|(found, expected)| (found - expected).abs() < 1e-3)
}

#[test]
fn three_ends_of_a_straight_road_and_an_arm_meet_at_a_tee_or_an_h_where_the_arm_slants() {
    // West, east and north: a straight road and an arm square to it, a tee. Its piece runs from
    // the origin half a road width, 17.5, north, towards the arm; each segment's end moves to the
    // tee's, half a road width out.
    let (junctions, ends) = junction(&[[-100.0, 0.0], [100.0, 0.0], [0.0, 100.0]]);
    assert_eq!(junctions.len(), 1);
    let tee = junctions[0];
    assert_eq!(tee.kind, PieceKind::Tee);
    assert_eq!(tee.start.point, Vec2::ZERO);
    assert!(near(tee.end.point, [0.0, 17.5]));
    assert_eq!(tee.start.count, -3);
    assert!(all_near(&ends, &[17.5; 3]));
    // Its piece: 1.03 road widths across and along, the arm's side from its place.
    let piece = tee.piece();
    assert!((piece.normal.length() - 35.0 * 1.03 / 2.0).abs() < 1e-3);
    assert!((piece.along.length() - (35.0 * 1.03 / 2.0 + 35.0 * 0.9 / 2.0)).abs() < 1e-3);

    // West, east and north-east: the arm 45° off square, past cos 60°, so an H. The straight
    // road's ends move 0.46 and 2.05 half widths out, 8.05 and 35.875, and the arm's 2.10, 36.75,
    // past the 50 the test looks within.
    let (junctions, ends) = junction(&[[-100.0, 0.0], [100.0, 0.0], [100.0, 100.0]]);
    assert_eq!(junctions.len(), 1);
    assert!(matches!(junctions[0].kind, PieceKind::H | PieceKind::HFlip));
    assert!(all_near(&ends, &[8.05, 35.875, 36.75]));
}

#[test]
fn three_ends_none_near_a_line_meet_at_a_y_and_four_at_a_four_way() {
    // North, south-west and south-east, 120° apart: a Y. Its stem's end moves on 0.55 half
    // widths, 9.625, and each leg's 1.1 out, 19.25.
    let (sin, cos) = libm::sincosf(30.0_f32.to_radians());
    let (junctions, ends) = junction(&[
        [0.0, 100.0],
        [-100.0 * cos, -100.0 * sin],
        [100.0 * cos, -100.0 * sin],
    ]);
    assert_eq!(junctions.len(), 1);
    assert_eq!(junctions[0].kind, PieceKind::Y);
    assert!(all_near(&ends, &[9.625, 19.25, 19.25]));
    assert!(((junctions[0].end.point - junctions[0].start.point).length() - 17.5).abs() < 1e-3);

    // North, east, south and west: a four-way, half a road width along its straight road and
    // east, 1.03 of its texture across; each end moved half a road width out.
    let (junctions, ends) = junction(&[[0.0, 100.0], [100.0, 0.0], [0.0, -100.0], [-100.0, 0.0]]);
    assert_eq!(junctions.len(), 1);
    let four = junctions[0];
    assert_eq!(four.kind, PieceKind::FourWay);
    assert_eq!((four.start.count, four.width_in_texture), (-4, 1.03));
    assert!(four.end.point.x >= 0.0);
    assert!((four.end.point.length() - 17.5).abs() < 1e-3);
    assert!(all_near(&ends, &[17.5; 4]));
}

#[test]
fn an_end_that_asks_for_a_join_fades_into_the_road_it_meets_and_stacks_over_it() {
    let sidewalk = RoadId(1);
    // A sidewalk from (−100, 0) ends at (0, 0), asking for a join, on a gravel road from
    // (0, −100) to (0, 100): within the road's box, 17.5 past its line, and on its line, within
    // 0.55 of its 35. The join runs along the gravel road turned a right angle towards the
    // sidewalk, (−200, 0), cut square, 24 · 0.95 = 22.8 across as the sidewalk is, so its scale
    // across is the sidewalk's 24; the sidewalk stacks over the gravel road.
    let mut walk = RoadSegment {
        road: sidewalk,
        width: 24.0,
        width_in_texture: 0.95,
        ..segment([-100.0, 0.0], [0.0, 0.0], false)
    };
    walk.end.join = true;
    let road = segment([0.0, -100.0], [0.0, 100.0], false);
    let buffer = RoadBuffer::chained(&[walk, road]);
    let joins: Vec<BufferRoad> = buffer
        .roads
        .iter()
        .filter(|piece| piece.kind == PieceKind::AlphaJoin)
        .copied()
        .collect();
    assert_eq!(joins.len(), 1);
    let join = joins[0];
    assert_eq!((join.road, join.start.point), (sidewalk, Vec2::ZERO));
    assert!(near(join.end.point, [-200.0, 0.0]));
    assert!((join.width_in_texture - 24.0).abs() < 1e-3);
    assert_eq!(buffer.stacks, BTreeMap::from([(sidewalk, 1)]));
    // The sidewalk, listed first as read last, draws after the gravel road it stacks over.
    let listed = RoadBuffer {
        types: vec![sidewalk, RoadId(0)],
        ..RoadBuffer::chained(&[walk, road])
    };
    assert_eq!(listed.drawing_order(), [1, 0]);
    assert_eq!(buffer.pieces().joins, 1);
    // Its piece: 48 128ths of a road width along, 24 · 0.375 = 9, and 1 + 8 128ths of its width
    // across, 25.5, its `v` scaled by that width.
    let piece = join.piece();
    assert!((piece.along.length() - 9.0).abs() < 1e-3);
    assert!((piece.normal.length() - 25.5).abs() < 1e-3);
    assert!(near(piece.scale, [24.0, 24.0]));

    // Alone, the join runs 100 back along its own road, and stacks nothing; a gravel road's
    // end there asks for none.
    let buffer = RoadBuffer::chained(&[walk]);
    let join = buffer.roads[1];
    assert_eq!(join.kind, PieceKind::AlphaJoin);
    assert!(near(join.end.point, [-100.0, 0.0]));
    assert!(buffer.stacks.is_empty());
    let alone = RoadBuffer::chained(&[road]);
    assert_eq!(alone.roads.len(), 1);
}

/// A turn takes its sine and cosine from `libm` in software, so every OS gives the same bits:
/// (1, 0) turned by 1 radian is (cos 1, sin 1), each rounded to the nearest `f32`.
#[test]
fn a_turn_gives_the_same_bits_on_every_os() {
    let turned = plane::rotate(Vec2::X, 1.0);
    assert_eq!(
        [turned.x.to_bits(), turned.y.to_bits()],
        [0x3f0a_5140, 0x3f57_6aa4]
    );
}
