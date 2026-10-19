use campfire_codec::Binary;
use campfire_math::FloorRoot;
use campfire_math::SplitMix64;

use super::*;
use crate::placed::Placed;
use crate::shape::Shape;

impl Frame {
    fn holds(&self, off: Flat) -> bool {
        let area = self.quarter_area().cast_unsigned();
        self.slabs(off)
            .iter()
            .all(|value| value.unsigned_abs() <= area)
    }

    /// Whether `off` lies inside the open box, off its edges.
    fn holds_open(&self, off: Flat) -> bool {
        let area = self.quarter_area().cast_unsigned();
        self.slabs(off)
            .iter()
            .all(|value| value.unsigned_abs() < area)
    }
}

impl Closer {
    /// Whether `off` lies closer than the reach to the box, as its row's test decides.
    pub(crate) fn holds_point(&self, off: Flat) -> bool {
        self.holds(&self.row(off[1]), off[0])
    }
}

impl BodyBox {
    /// Whether `position` lies inside the box at `center` or on its edge.
    fn holds(&self, center: Vec3, position: Vec3) -> bool {
        self.frame().holds(flat(position) - flat(center))
    }
}

fn num(text: &str) -> Num {
    text.parse().unwrap()
}

fn at(x: &str, z: &str) -> Vec3 {
    Vec3::new(num(x), Num::ZERO, num(z))
}

fn made(width: &str, height: &str, angle: &str) -> BodyBox {
    BodyBox::new([num(width), num(height)], num(angle)).unwrap()
}

fn halves(body: &BodyBox) -> [[i64; 2]; 2] {
    body.half_edges().map(|edge| edge.map(Num::to_bits))
}

/// The box's corners at `center`, as a polygon.
fn polygon(body: &BodyBox, center: Vec3) -> Polygon {
    let at = flat(center);
    let points = body.frame().corners().map(|corner| {
        (corner + at)
            .to_array()
            .map(|bits| Num::from_bits(i64::try_from(bits).unwrap()))
    });
    Polygon::new(points).unwrap()
}

/// The distance from `position` to the nearest point of `body` at `center`, in bits; 0 inside.
fn distance(body: &BodyBox, center: Vec3, position: Vec3) -> i128 {
    let frame = body.frame();
    frame
        .nearest_to(
            flat(position) - flat(center),
            &mut HalfDirections::new(frame),
        )
        .distance
}

fn flat_vec3(off: Flat) -> Vec3 {
    Vec3::new(Num::from_bits(off[0]), Num::ZERO, Num::from_bits(off[1]))
}

/// Where the path from `start` along `path` crosses the segment from `a` to `b`, by the two
/// lines' cross products: its share of the path, `t / den`, as `[t, den]` with `den` positive;
/// none for parallel lines or no crossing.
fn crossing(start: Flat, path: Flat, a: Flat, b: Flat) -> Option<[i128; 2]> {
    let edge = b - a;
    let den = path.cross(edge);
    if den == 0 {
        return None;
    }
    let to_a = a - start;
    let sign = den.signum();
    let (t, u, den) = (to_a.cross(edge) * sign, to_a.cross(path) * sign, den.abs());
    let within = |num: i128| 0 <= num && num <= den;
    (within(t) && within(u)).then_some([t, den])
}

#[test]
fn a_box_turns_exactly_by_quarters_and_rounds_once_elsewhere() {
    let one = Num::ONE.to_bits();
    // 4 × 2 m: half edges (2, 0) and (0, 1), turned a quarter at a time from x towards −z, as a
    // facing turns, and the same a whole turn on, or before.
    let quarters = [
        ("0", [[2, 0], [0, 1]]),
        ("90", [[0, -2], [1, 0]]),
        ("180", [[-2, 0], [0, -1]]),
        ("270", [[0, 2], [-1, 0]]),
        ("360", [[2, 0], [0, 1]]),
        ("-90", [[0, 2], [-1, 0]]),
        ("450", [[0, -2], [1, 0]]),
    ];
    for (angle, meters) in quarters {
        let expected = meters.map(|edge| edge.map(|m: i64| m * one));
        assert_eq!(halves(&made("4", "2", angle)), expected, "{angle}°");
    }
    // Half a size rounds to nearest, ties to even: 2²⁴ + 1 bits halve to the even 2²³, and
    // 2²⁴ + 3 to 2²³ + 2.
    let odd = |bits: i64| BodyBox::new([Num::from_bits(bits), Num::ONE], Num::ZERO).unwrap();
    assert_eq!(halves(&odd(one + 1))[0][0], one / 2);
    assert_eq!(halves(&odd(one + 3))[0][0], one / 2 + 2);
    // At 45°, a 2 m side's half edge is the cosine and the negated sine themselves: within a bit
    // of cos 45° × 2²⁴ = 11 863 283.2, as `sin_cos` is within 0.501 of a bit of the sine of the
    // rounded angle, itself within 2⁻²⁵ rad of π/4. The other half edge is the first turned a
    // quarter, from the same products.
    let turned = halves(&made("2", "2", "45"));
    let [along, across] = turned[0];
    assert!((along - 11_863_283).abs() <= 1, "{along}");
    assert!((across + 11_863_283).abs() <= 1, "{across}");
    assert_eq!(turned[1], [-turned[0][1], turned[0][0]]);
}

#[test]
fn a_box_has_the_least_size_and_the_longest_diagonal() {
    let least = BodyBox::MIN_SIZE;
    let below = Num::from_bits(least.to_bits() - 1);
    assert!(BodyBox::new([least, least], num("30")).is_some());
    assert!(BodyBox::new([below, Num::ONE], Num::ZERO).is_none());
    assert!(BodyBox::new([Num::ONE, Num::ZERO], Num::ZERO).is_none());
    assert!(BodyBox::new([-Num::ONE, Num::ONE], Num::ZERO).is_none());
    // A diagonal of 4,094 m holds a 4,094 m side only with no other; one of 4,093.9 and 1 m is
    // within it, 2 046.95² + 0.5² = 4 189 998.55 ≤ 2 047², and at 45° its corners stay within
    // 2,048 m.
    assert!(BodyBox::new([num("4094"), least], Num::ZERO).is_none());
    assert!(BodyBox::new([num("4094"), Num::ZERO], Num::ZERO).is_none());
    let longest = made("4093.9", "1", "45");
    let [a, b] = longest.frame().halves;
    let reach = (2048 * i128::from(Num::ONE.to_bits())).pow(2);
    assert!((a + b).dot(a + b) <= reach && (a - b).dot(a - b) <= reach);
}

#[test]
fn a_box_has_a_bound_and_decodes_only_as_a_box() {
    // A 4 × 2 box reaches √5 m from its position at every corner: its bound is that, rounded
    // up to a bit.
    let body = made("4", "2", "0");
    let five = 5 * (1_i128 << 48);
    let bound = i128::from(body.bound().to_bits());
    assert!(
        bound * bound >= five && (bound - 1) * (bound - 1) < five,
        "{bound}"
    );
    // Quarters turn it whole, so its bound stays.
    assert_eq!(made("4", "2", "90").bound(), body.bound());
    // A snapshot's box decodes to the same box, its bound derived again; half edges that turn
    // clockwise, lie flat, or reach past 2,048 m do not decode: (1 600, 1 300) m reaches
    // 2 061.55 m, past it, though each half edge is within it; (1 400, 1 400) m reaches
    // 1 979.90 m.
    let decoded: BodyBox = Binary::decode(&Binary::encode(&body)).unwrap();
    assert_eq!(decoded, body);
    let encode = |half: [[&str; 2]; 2]| Binary::encode(&half.map(|edge| edge.map(num)));
    for half in [
        [["0", "1"], ["2", "0"]],
        [["2", "0"], ["4", "0"]],
        [["1600", "0"], ["0", "1300"]],
    ] {
        assert!(
            Binary::decode::<BodyBox>(&encode(half)).is_err(),
            "{half:?}"
        );
    }
    // A component as large as a number holds is refused, with no product of it.
    let huge = Binary::encode(&[[Num::MAX, Num::ZERO], [Num::ZERO, Num::MAX]]);
    assert!(Binary::decode::<BodyBox>(&huge).is_err());
    assert!(Binary::decode::<BodyBox>(&encode([["2", "0"], ["0", "1"]])).is_ok());
    assert!(Binary::decode::<BodyBox>(&encode([["1400", "0"], ["0", "1400"]])).is_ok());
}

#[test]
fn a_box_holds_its_inside_and_its_edge() {
    let body = made("4", "2", "0");
    let center = at("10", "5");
    // A bit is 2⁻²⁴ m, 0.000000059604644775390625.
    for (x, z, inside) in [
        ("10", "5", true),
        ("12", "6", true),
        ("8", "4", true),
        ("11", "6", true),
        ("12", "6.000000059604644775390625", false),
        ("12.000000059604644775390625", "5", false),
    ] {
        assert_eq!(body.holds(center, at(x, z)), inside, "({x}, {z})");
    }
    // Turned a quarter, it reaches 1 m along x and 2 m along z.
    let upright = made("4", "2", "90");
    assert!(upright.holds(center, at("11", "7")));
    assert!(!upright.holds(center, at("12", "6")));
}

#[test]
fn a_distance_runs_to_the_nearest_edge_or_corner() {
    let body = made("4", "2", "0");
    let center = at("0", "0");
    let bit = Num::EPSILON;
    // 3 m from the edge at x = 2: within a reach of 3, not of 3 less a bit.
    assert_eq!(
        body.nearest(center, at("5", "0"), num("3")),
        Ordering::Equal
    );
    assert_eq!(
        body.nearest(center, at("5", "0"), num("3") - bit),
        Ordering::Greater
    );
    // From the corner (2, 1) to (5, 4), √18 m: past 4, within 5.
    assert_eq!(
        body.nearest(center, at("5", "4"), num("4")),
        Ordering::Greater
    );
    assert_eq!(body.nearest(center, at("5", "4"), num("5")), Ordering::Less);
    // Inside, below 0, as a signed distance measures it: closer than a reach of 0. On the
    // edge at x = 2, 0: a reach of 0 meets it.
    assert_eq!(
        body.nearest(center, at("1", "0.5"), Num::ZERO),
        Ordering::Less
    );
    assert_eq!(
        body.nearest(center, at("2", "0.5"), Num::ZERO),
        Ordering::Equal
    );
    // Two distances compare exactly: 3 m from the edge at x = 2 and from the one at z = 1;
    // a bit further from the second is further.
    let side = distance(&body, center, at("5", "0"));
    assert_eq!(side, distance(&body, center, at("0", "4")));
    assert!(distance(&body, center, at("0", "4.000000059604644775390625")) > side);
    // √18 from the corner against 4 from an edge: 18 > 16.
    assert!(distance(&body, center, at("5", "4")) > distance(&body, center, at("0", "5")));
    // Turned a quarter, the edge at x = 1 lies 3 m from (4, 0).
    let upright = made("4", "2", "90");
    assert_eq!(
        upright.nearest(center, at("4", "0"), num("3")),
        Ordering::Equal
    );
}

#[test]
fn the_nearest_point_lies_on_the_nearest_edge_or_corner() {
    let body = made("4", "2", "0");
    let center = at("0", "0");
    assert_eq!(body.nearest_point(center, at("5", "0")), at("2", "0"));
    assert_eq!(body.nearest_point(center, at("5", "4")), at("2", "1"));
    assert_eq!(body.nearest_point(center, at("1", "0.5")), at("1", "0.5"));
    // Turned 45°, the point nearest one beside its edge lies on that edge, within a bit, and as
    // far from it as the box is, within the bit it rounds by.
    let turned = made("4", "2", "45");
    let probe = at("3", "-3");
    let point = turned.nearest_point(center, probe);
    assert_eq!(turned.nearest(center, point, Num::EPSILON), Ordering::Less);
    // Each coordinate rounds by half a bit at most, so the way to the point lies within a bit
    // of the exact distance, and the box's distance, rounded, within half a bit more.
    let way = (flat(point) - flat(probe))
        .length_squared()
        .nearest_root()
        .cast_signed();
    assert!((distance(&turned, center, probe) - way).abs() <= 2);
}

#[test]
fn twice_the_scale_sees_halves_of_a_bit() {
    let body = made("2", "2", "0");
    let twice = |meters: i128, halves: i128| (meters << 25) + halves;
    // 1.5 m along x lies half a meter off the edge: not closer than half a meter, closer than a
    // bit more.
    let off = Flat::new(twice(1, 1 << 24), 0);
    assert!(!body.closer_twice(Num::HALF).holds_point(off));
    assert!(body.closer_twice(Num::HALF + Num::EPSILON).holds_point(off));
    // Half a bit past the edge: closer than a bit, not than nothing. On the edge, at 0, the
    // same; half a bit inside it, inside the open box, closer than nothing too.
    for (halves, at_zero) in [(1, false), (0, false), (-1, true)] {
        let off = Flat::new(twice(1, halves), 0);
        assert!(body.closer_twice(Num::EPSILON).holds_point(off));
        assert_eq!(body.closer_twice(Num::ZERO).holds_point(off), at_zero);
    }
    // A square of 1 m centered 1.5 m along x touches the box's edge; half a bit nearer, it
    // overlaps.
    let half = 1 << 24;
    assert!(!body.overlaps_square_twice(half)(Flat::new(
        twice(1, 1 << 24),
        0
    )));
    assert!(body.overlaps_square_twice(half)(Flat::new(
        twice(1, (1 << 24) - 1),
        0
    )));
}

#[test]
fn a_path_comes_nearest_where_it_enters_or_at_its_first_nearest_point() {
    let body = made("4", "2", "0");
    let center = at("0", "0");
    let approach = |from: Vec3, to: Vec3, reach: &str| body.approach(center, from, to, num(reach));
    // Through it along x from -10: it enters at x = -2, 8 m along the path's 20 m, and into
    // the open box, so closer than a reach of 0; from the other end, at x = 2, 8 m in too.
    // Along the edge at z = 1 it meets the closed box at x = -2 and never enters the open
    // one: at 0.
    let through = approach(at("-10", "0"), at("10", "0"), "0");
    assert_eq!(
        through,
        Approach {
            nearest: Ordering::Less,
            place: num("8")
        }
    );
    assert_eq!(
        approach(at("-10", "1"), at("10", "1"), "0"),
        Approach {
            nearest: Ordering::Equal,
            place: num("8")
        }
    );
    assert_eq!(
        approach(at("-10", "0"), at("10", "0"), "1").nearest,
        Ordering::Less
    );
    assert_eq!(approach(at("10", "0"), at("-10", "0"), "0").place, num("8"));
    // From inside: at its start.
    assert_eq!(approach(at("0", "0"), at("10", "0"), "0").place, Num::ZERO);
    // Along z = 3, 2 m off the edge at z = 1 from x = -2 to 2: the first of that stretch.
    let beside = approach(at("-10", "3"), at("10", "3"), "2");
    assert_eq!(
        beside,
        Approach {
            nearest: Ordering::Equal,
            place: num("8")
        }
    );
    // As a yes or no through the shape, which tests the spans first: touching at the reach is
    // not closer, a bit more reach is; through it, any reach, 0 too; along its edge at z = 1,
    // any reach but 0, and from inside to its edge, 0 too; along x = 4, 2 m off the end at
    // x = 2, the spans alone tell.
    let as_shape = Shape::Box(body);
    let comes = |from: Vec3, to: Vec3, reach: Num| {
        let comes = Placed::new(center, as_shape).comes_closer(from, to, reach);
        assert_eq!(comes, body.comes_closer(center, from, to, reach));
        comes
    };
    let bit = Num::EPSILON;
    assert!(!comes(at("-10", "3"), at("10", "3"), num("2")));
    assert!(comes(at("-10", "3"), at("10", "3"), num("2") + bit));
    assert!(comes(at("-10", "0"), at("10", "0"), Num::ZERO));
    assert!(!comes(at("-10", "1"), at("10", "1"), Num::ZERO));
    assert!(comes(at("-10", "1"), at("10", "1"), bit));
    assert!(comes(at("0", "0"), at("0", "1"), Num::ZERO));
    assert!(!comes(at("4", "-5"), at("4", "5"), num("2")));
    assert!(comes(at("4", "-5"), at("4", "5"), num("2") + bit));
    let short = body.approach(
        center,
        at("-10", "3"),
        at("10", "3"),
        num("2") - Num::EPSILON,
    );
    assert_eq!(short.nearest, Ordering::Greater);
    // From (5, 5) to (10, 0), the corner (2, 1) is nearest a tenth of the way, 1/√2 m along,
    // 2²⁴/√2 = 11,863,283.2 bits, and 7/√2 m off, 4.95 m: within 5, past 4.9.
    let corner = approach(at("5", "5"), at("10", "0"), "5");
    assert_eq!(
        corner,
        Approach {
            nearest: Ordering::Less,
            place: Num::from_bits(11_863_283)
        }
    );
    assert_eq!(
        approach(at("5", "5"), at("10", "0"), "4.9").nearest,
        Ordering::Greater
    );
    // A path of no length is a point.
    let still = approach(at("5", "0"), at("5", "0"), "3");
    assert_eq!(
        still,
        Approach {
            nearest: Ordering::Equal,
            place: Num::ZERO
        }
    );
    // Past its end: the end is nearest, 1 m short of the edge, 7 m along.
    let short_of = approach(at("-10", "0"), at("3", "0"), "1");
    assert_eq!(short_of.place, num("8"));
    let stops = approach(at("10", "0"), at("3", "0"), "1");
    assert_eq!(
        stops,
        Approach {
            nearest: Ordering::Equal,
            place: num("7")
        }
    );
}

#[test]
fn a_body_is_pushed_out_of_a_box() {
    let body = made("4", "2", "0");
    let center = at("0", "0");
    let point = |x: &str, z: &str| at(x, z);
    let bit = Num::EPSILON;
    let past = |x: &str, z: &str, by: [Num; 2]| {
        let at = point(x, z);
        Vec3::new(at.x + by[0], at.y, at.z + by[1])
    };
    // Half a meter into the edge at x = 2, from outside and from inside: out to touch it, and
    // `PUSH_SLACK`, a bit, past it, as the edge's normal is exact.
    assert_eq!(
        body.push_out(center, point("2.5", "0"), Num::ONE),
        Some(past("3", "0", [bit, Num::ZERO]))
    );
    assert_eq!(
        body.push_out(center, point("1.5", "0"), Num::ONE),
        Some(past("3", "0", [bit, Num::ZERO]))
    );
    // Off the corner (2, 1) by (0.5, 0.5) m: 2²³·√2 = 11 863 283.2 bits from it, measured
    // 11 863 283, so out by 2²⁴ − 11 863 283 + 1 = 4 913 934 bits along the diagonal, 3 474 676.07
    // along each axis, rounded up to 3 474 677: it ends 11 863 285 bits from the corner along
    // each axis, 16 777 218.6 bits away, 2.6 bits past touching.
    let off_corner = body
        .push_out(center, point("2.5", "1.5"), Num::ONE)
        .unwrap();
    assert_eq!(off_corner.x.to_bits(), 45_417_717);
    assert_eq!(off_corner.z.to_bits(), 28_640_501);
    assert_eq!(body.push_out(center, off_corner, Num::ONE), None);
    // At the center of a square, every edge is as near: out through the first, at z = -1.
    let square = made("2", "2", "0");
    assert_eq!(
        square.push_out(center, point("0", "0"), Num::ONE),
        Some(past("0", "-2", [Num::ZERO, -bit]))
    );
    // Clear of it, and touching it, no push.
    assert_eq!(body.push_out(center, point("3.5", "0"), Num::ONE), None);
    assert_eq!(body.push_out(center, point("3", "0"), Num::ONE), None);
    // Turned a quarter, its edge at z = 2.
    let upright = made("4", "2", "90");
    assert_eq!(
        upright.push_out(center, point("0", "2.5"), Num::ONE),
        Some(past("0", "3", [Num::ZERO, bit]))
    );
    // Turned 30°, from x towards −z: off the edge from a to a − b at 1.25·a, off the corner
    // a − b by (0.25, −0.25) m, and inside at (1, 0) m, 0.5 m from the edge from −a + b to a + b,
    // the nearest: each ends apart, within three bits of touching.
    let turned = made("4", "2", "30");
    let bits = |x: i64, z: i64| Vec3::new(Num::from_bits(x), Num::ZERO, Num::from_bits(z));
    for from in [
        bits(36_323_738, -20_971_520),
        bits(24_864_686, -35_501_015),
        bits(16_777_216, 0),
    ] {
        assert!(
            turned.push_out(center, from, Num::ONE).is_some(),
            "{from:?}"
        );
        check_push(&turned, center, from, Num::ONE);
    }
    // The height stays.
    let high = Vec3::new(num("2.5"), num("7"), Num::ZERO);
    assert_eq!(body.push_out(center, high, Num::ONE).unwrap().y, num("7"));
}

#[test]
fn boxes_overlap_when_their_insides_meet() {
    let body = made("2", "2", "0");
    let origin = at("0", "0");
    let bit = Num::EPSILON;
    let beside = |x: Num| Vec3::new(x, Num::ZERO, Num::ZERO);
    // Side by side, they touch and do not overlap; a bit closer, they do.
    assert!(!body.overlaps(origin, &body, beside(num("2"))));
    assert!(body.overlaps(origin, &body, beside(num("2") - bit)));
    // Touching, at 0; overlapping, closer than a reach of 0.
    assert_eq!(
        body.nearest_box(origin, &body, beside(num("2")), Num::ZERO),
        Ordering::Equal
    );
    assert_eq!(
        body.nearest_box(origin, &body, beside(num("2") - bit), Num::ZERO),
        Ordering::Less
    );
    // A 4 × 2 box turned a quarter at x = 2 spans x from 1 to 3, and z from -2 to 2.
    let upright = made("4", "2", "90");
    assert!(!body.overlaps(origin, &upright, beside(num("2"))));
    assert!(body.overlaps(origin, &upright, beside(num("2") - bit)));
    assert!(upright.overlaps(beside(num("2") - bit), &body, origin));
    // 3 m apart along x, and √18 m apart at the corners (1, 1) and (4, 4).
    assert_eq!(
        body.nearest_box(origin, &body, at("5", "0"), num("3")),
        Ordering::Equal
    );
    assert_eq!(
        body.nearest_box(origin, &body, at("5", "5"), num("4")),
        Ordering::Greater
    );
    assert_eq!(
        body.nearest_box(origin, &body, at("5", "5"), num("5")),
        Ordering::Less
    );
    // A box inside another overlaps it.
    let small = made("0.5", "0.5", "30");
    assert!(body.overlaps(origin, &small, at("0.2", "0.1")));
}

#[test]
fn a_box_overlaps_a_polygon_its_inside_meets() {
    let body = made("4", "2", "0");
    let origin = at("0", "0");
    let shape = |points: &[(&str, &str)]| {
        Polygon::new(points.iter().map(|&(x, z)| [num(x), num(z)])).unwrap()
    };
    let cases: [(&[(&str, &str)], bool); 7] = [
        // A corner on the edge at x = 2, and an edge along it: touching.
        (&[("2", "0"), ("4", "-1"), ("4", "1")], false),
        (&[("2", "-1"), ("4", "0"), ("2", "1")], false),
        // A corner a meter inside.
        (&[("1", "0"), ("4", "-1"), ("4", "1")], true),
        // Around the box, and inside it.
        (
            &[("-10", "-10"), ("10", "-10"), ("10", "10"), ("-10", "10")],
            true,
        ),
        (&[("-0.5", "-0.5"), ("0.5", "-0.5"), ("0", "0.5")], true),
        // An edge from corner (-2, -1) to corner (2, 1) and on: no edge crosses another, and
        // no corner of either lies inside the other, but the edge runs through the inside.
        (&[("-4", "-2"), ("4", "2"), ("4", "-10")], true),
        // The box itself.
        (&[("-2", "-1"), ("2", "-1"), ("2", "1"), ("-2", "1")], true),
    ];
    for (points, overlaps) in cases {
        assert_eq!(
            body.overlaps_polygon(origin, &shape(points)),
            overlaps,
            "{points:?}"
        );
    }
}

/// Draws for the random boxes: sizes, angles, offsets and reaches, every run the same, each
/// length `scale` times its meters.
struct Draws {
    words: SplitMix64,
    scale: i64,
}

impl Draws {
    const ONE: i64 = Num::ONE.to_bits();

    /// A meter, scaled.
    const fn meter(&self) -> i64 {
        Self::ONE * self.scale
    }

    fn bits(&mut self, low: i64, high: i64) -> i64 {
        let span = u64::try_from(high - low).unwrap();
        low + i64::try_from(self.words.next_u64() % span).unwrap()
    }

    fn size(&mut self) -> Num {
        let high = 20 * self.meter();
        Num::from_bits(self.bits(BodyBox::MIN_SIZE.to_bits(), high))
    }

    /// A whole number of quarters, or any angle.
    fn angle(&mut self, quarters: bool) -> Num {
        if quarters {
            Num::from_bits(self.bits(0, 4) * 90 * Self::ONE)
        } else {
            Num::from_bits(self.bits(0, 360 * Self::ONE))
        }
    }

    fn body(&mut self, quarters: bool) -> BodyBox {
        let size = [self.size(), self.size()];
        BodyBox::new(size, self.angle(quarters)).unwrap()
    }

    /// A point within `span` meters of `from` along each axis.
    fn near(&mut self, from: Vec3, span: i64) -> Vec3 {
        let meter = self.meter();
        let mut axis = || Num::from_bits(self.bits(-span * meter, span * meter));
        let offset = Vec3::new(axis(), Num::ZERO, axis());
        from + offset
    }

    /// A reach below `meters`.
    fn reach(&mut self, meters: i64) -> Num {
        let high = meters * self.meter();
        Num::from_bits(self.bits(0, high))
    }
}

/// How the nearest of `paths`' approaches, each a path and an offset from its start, lies
/// against `reach`, by `Approach`, which knows nothing of boxes.
fn by_approaches(paths: impl Iterator<Item = (Flat, Flat)>, reach: Num) -> Ordering {
    paths
        .map(|(path, off)| {
            let (path, off) = (flat_vec3(path), flat_vec3(off));
            let nearest = Approach::of(path, off, reach).nearest;
            assert_eq!(Approach::nearest_of(path, off, reach), nearest);
            nearest
        })
        .min()
        .unwrap()
}

/// The approaches of `point`, from the box's position, to each of the box's edges.
fn to_edges(body: &BodyBox, point: Flat) -> impl Iterator<Item = (Flat, Flat)> {
    let corners = body.frame().corners();
    (0..4).map(move |edge| {
        let start = corners[edge];
        ((corners[(edge + 1) % 4] - start), (point - start))
    })
}

/// A point's distance against a reach agrees with its approaches to the edges, and whether
/// the box holds it with the polygon of its corners.
fn check_point(body: &BodyBox, center: Vec3, probe: Vec3, reach: Num) {
    let holds = body.holds(center, probe);
    assert_eq!(
        holds,
        polygon(body, center).holds_point([probe.x, probe.z]),
        "{body:?} {probe:?}"
    );
    let expected = if body.frame().holds_open(flat(probe) - flat(center)) {
        Ordering::Less
    } else if holds {
        Num::ZERO.cmp(&reach)
    } else {
        by_approaches(to_edges(body, flat(probe) - flat(center)), reach)
    };
    assert_eq!(
        body.nearest(center, probe, reach),
        expected,
        "{body:?} {probe:?}"
    );
    let reach_bits = i128::from(reach.to_bits());
    let within = Closer::new(body.frame(), reach_bits).holds_point(flat(probe) - flat(center));
    assert_eq!(within, expected == Ordering::Less, "{body:?} {probe:?}");
    // At both scales the shared terms decide as each edge's own measures do.
    let off = flat(probe) - flat(center);
    for (frame, off, reach) in [
        (body.frame(), off, reach_bits),
        (body.frame_twice(), off * 2, reach_bits * 2),
    ] {
        let by_edges = by_edges(frame, off, reach);
        let edges = Edges::of(frame);
        let against = edges.against(off, &mut LinesOnDemand::new(&edges, Reach::of(reach)));
        assert_eq!(against, by_edges, "{body:?} {probe:?}");
        let closer = Closer::new(frame, reach).holds_point(off);
        assert_eq!(closer, by_edges == Ordering::Less, "{body:?} {probe:?}");
    }
}

#[test]
fn a_distance_near_the_reach_decides_as_the_direction_measures_it() {
    // Every point within 3 units of the point the reach straight out from each edge's middle, of
    // boxes at awkward angles, at both scales: where the lines' bounds leave the measure to the
    // direction, and either side of them, against each edge's own measure.
    for angle in ["0", "1", "17.3", "45", "89.9", "123.4", "200", "271"] {
        let body = made("3.7", "1.3", angle);
        for frame in [body.frame(), body.frame_twice()] {
            let corners = frame.corners();
            for reach in [0, 1, 2, 5, 1000, 1 << 20, 3 << 23] {
                let edges = Edges::of(frame);
                let closer = Closer::new(frame, reach);
                for edge in 0..4 {
                    let (start, end) = (corners[edge], corners[(edge + 1) % 4]);
                    let along = (end - start).to_array();
                    // The outward normal of a counterclockwise edge, scaled to the reach.
                    let normal = [along[1], -along[0]];
                    let length = (normal[0] * normal[0] + normal[1] * normal[1])
                        .cast_unsigned()
                        .floor_root()
                        .cast_signed();
                    let middle = start.to_array();
                    let target = [0, 1]
                        .map(|axis| middle[axis] + along[axis] / 2 + normal[axis] * reach / length);
                    for dx in -3..=3 {
                        for dz in -3..=3 {
                            let off = Flat::new(target[0] + dx, target[1] + dz);
                            let expected = by_edges(frame, off, reach);
                            let mut lines = LinesOnDemand::new(&edges, Reach::of(reach));
                            assert_eq!(edges.against(off, &mut lines), expected, "{angle} {reach}");
                            let holds = closer.holds_point(off);
                            assert_eq!(holds, expected == Ordering::Less, "{angle} {reach}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn a_path_past_a_corner_near_the_reach_decides_as_the_direction_measures_it() {
    // Level paths 20 m long, either side of the reach above a tilted box's top corner, which
    // is the box's nearest point to them: where the path's bounds leave the corner's measure to
    // its direction, and either side of them.
    let center = at("0", "0");
    for angle in ["17.3", "45", "123.4"] {
        let body = made("3.7", "1.3", angle);
        let top = body
            .frame()
            .corners()
            .into_iter()
            .max_by_key(|corner| corner[1])
            .unwrap();
        for reach in [1, 2, 5, 1000, 1 << 20] {
            for gap in -3..=3 {
                let z = top[1] + reach + gap;
                let point = |x: i64| Vec3::new(Num::from_bits(x), Num::ZERO, Num::from_bits(z));
                let (from, to) = (point(top[0] - (10 << 24)), point(top[0] + (10 << 24)));
                let reach = Num::from_bits(reach);
                check_path(&body, center, from, to, reach);
            }
        }
    }
}

/// How the distance from `off` to `frame` lies against `reach`, each edge's measure from its own
/// corners and direction: inside the open box, closer than every reach; on its edge, 0; else the
/// nearest of the edges' orders.
fn by_edges(frame: Frame, off: Flat, reach: i128) -> Ordering {
    if frame.holds_open(off) {
        return Ordering::Less;
    }
    if frame.holds(off) {
        return 0.cmp(&reach);
    }
    let (corners, directions) = (frame.corners(), frame.directions());
    let reach = reach.cast_unsigned();
    (0..4)
        .map(|edge| {
            let (start, end) = (corners[edge], corners[(edge + 1) % 4]);
            let along_edge = end - start;
            let along = (off - start).dot(along_edge);
            if along <= 0 {
                (off - start).length_squared().nearest_root_cmp(reach)
            } else if along >= along_edge.dot(along_edge) {
                (off - end).length_squared().nearest_root_cmp(reach)
            } else {
                let apart = directions[edge % 2].apart((off - start).to_array());
                apart.cast_unsigned().cmp(&reach)
            }
        })
        .min()
        .unwrap()
}

/// A path's approach agrees with its crossings of the edges where it meets the box, and else
/// with the approaches of its ends to the edges and of the corners to it.
fn check_path(body: &BodyBox, center: Vec3, from: Vec3, to: Vec3, reach: Num) {
    let approach = body.approach(center, from, to, reach);
    let (start, path) = ((flat(from) - flat(center)), (flat(to) - flat(from)));
    let case = format!("{body:?} {from:?} {to:?} {reach:?}");
    let comes_closer = body.comes_closer(center, from, to, reach);
    assert_eq!(comes_closer, approach.nearest == Ordering::Less, "{case}");
    let by_shape = Placed::new(center, Shape::Box(*body)).comes_closer(from, to, reach);
    assert_eq!(by_shape, comes_closer, "{case}");
    let length = Direction::of(path.to_array()).map_or(0, |direction| direction.length());
    let meets = body.frame().meets_path(start, path, Closure::Closed);
    assert_eq!(
        meets,
        body.frame().entry(start, path, length).is_some(),
        "{case}"
    );
    let corners = body.frame().corners();
    // The first crossing of an edge, by its share of the path, at its place along it: each
    // rounding, of the length and of the place, is half a bit, so within a bit of it.
    let enters = (0..4)
        .filter_map(|edge| crossing(start, path, corners[edge], corners[(edge + 1) % 4]))
        .map(|[t, den]| Rounding::NearestEven.divide(t * length, den))
        .min();
    let enters = if body.holds(center, from) {
        Some(0)
    } else {
        enters
    };
    if let Some(enters) = enters {
        let place = i128::from(approach.place.to_bits());
        assert!((place - enters).abs() <= 1, "{case} {place} {enters}");
        let inside = body.frame().meets_path(start, path, Closure::Open) && reach >= Num::ZERO;
        let expected = if inside {
            Ordering::Less
        } else {
            Num::ZERO.cmp(&reach)
        };
        assert_eq!(approach.nearest, expected, "{case}");
        return;
    }
    let ends = to_edges(body, start).chain(to_edges(body, start + path));
    let tips = corners.into_iter().map(|corner| (path, (corner - start)));
    let expected = by_approaches(ends.chain(tips), reach);
    assert_eq!(
        approach.nearest, expected,
        "{body:?} {from:?} {to:?} {reach:?}"
    );
}

/// A body pushed out ends apart from the box, so a second push finds no overlap, and within
/// three bits of touching: `PUSH_SLACK`, the measure's half bit, and under a bit for the rounding
/// of each coordinate; the measure of the end takes half a bit more. One left alone did not
/// overlap it.
fn check_push(body: &BodyBox, center: Vec3, probe: Vec3, radius: Num) {
    let four_bits = Num::from_bits(4);
    match body.push_out(center, probe, radius) {
        Some(moved) => {
            let case = format!("{body:?} {center:?} {probe:?} {radius:?}");
            assert_eq!(body.push_out(center, moved, radius), None, "{case}");
            assert_ne!(
                body.nearest(center, moved, radius),
                Ordering::Less,
                "{case}"
            );
            assert_eq!(
                body.nearest(center, moved, radius + four_bits),
                Ordering::Less,
                "{case}"
            );
        }
        None => assert_ne!(body.nearest(center, probe, radius), Ordering::Less),
    }
}

/// Two boxes' overlap agrees both ways round and with one box and the other's polygon, and
/// their distance against a reach with each corner's approaches to the other's edges. Whether
/// they overlap.
fn check_pair(body: &BodyBox, center: Vec3, other: &BodyBox, position: Vec3, reach: Num) -> bool {
    let overlaps = body.overlaps(center, other, position);
    assert_eq!(overlaps, other.overlaps(position, body, center));
    let by_polygon = body.overlaps_polygon(center, &polygon(other, position));
    assert_eq!(
        overlaps, by_polygon,
        "{body:?} {center:?} {other:?} {position:?}"
    );
    let expected = if overlaps && reach >= Num::ZERO {
        Ordering::Less
    } else if overlaps {
        Num::ZERO.cmp(&reach)
    } else {
        let apart = flat(position) - flat(center);
        let ours = body
            .frame()
            .corners()
            .into_iter()
            .flat_map(|corner| to_edges(other, corner - apart));
        let theirs = other
            .frame()
            .corners()
            .into_iter()
            .flat_map(|corner| to_edges(body, corner + apart));
        by_approaches(ours.chain(theirs), reach)
    };
    assert_eq!(body.nearest_box(center, other, position, reach), expected);
    overlaps
}

/// Random boxes, points and paths agree with independent tests: the polygon of a box's
/// corners, `Approach` along its edges, and crossings by cross products. One box in four is
/// turned by whole quarters.
#[test]
fn random_boxes_agree_with_their_polygons_and_edges() {
    // At meters, and at 100 times them: boxes up to 2,000 m a side, bodies up to 300 m.
    for scale in [1, 100] {
        random_boxes(scale);
    }
}

fn random_boxes(scale: i64) {
    let mut draws = Draws {
        words: SplitMix64::new(0xB0C5),
        scale,
    };
    let origin = Vec3::ZERO;
    let (mut overlapping, mut apart) = (0, 0);
    for round in 0..400 {
        let quarters = round % 4 == 0;
        let body = draws.body(quarters);
        let center = draws.near(origin, 50);
        for _ in 0..8 {
            let (probe, reach) = (draws.near(center, 15), draws.reach(10));
            check_point(&body, center, probe, reach);
        }
        // Paths of any length, and of none, through the box or by it, against reaches from 0.
        for _ in 0..4 {
            let (from, to, reach) = (
                draws.near(center, 20),
                draws.near(center, 20),
                draws.reach(5),
            );
            check_path(&body, center, from, to, reach);
            check_path(&body, center, from, from, reach);
            check_path(&body, center, from, to, Num::ZERO);
        }
        let high = 3 * draws.meter();
        let radius = Num::from_bits(draws.bits(BodyBox::MIN_SIZE.to_bits(), high));
        check_push(&body, center, draws.near(center, 12), radius);
        let other = draws.body(quarters);
        let (at, reach) = (draws.near(center, 25), draws.reach(10));
        if check_pair(&body, center, &other, at, reach) {
            overlapping += 1;
        } else {
            apart += 1;
        }
    }
    // Both outcomes came up often enough to test each.
    assert!(overlapping > 50 && apart > 50, "{overlapping} {apart}");
}

#[test]
fn a_body_at_the_bound_is_pushed_out_of_the_longest_box() {
    // The longest box, turned 45° from x towards −z, and a body of 2,048 m into it: each push
    // ends apart, within three bits of touching. The box's half edges are 2 046.95 m along
    // (√½, −√½) and 0.5 m along (√½, √½), so its long edge's line lies 0.5 m from the center,
    // its short edge's 2 046.95 m, and a corner at (1 447.06, −1 447.77) m: 1 447.41 ∓ 0.35,
    // −(1 447.41 + 0.35).
    let longest = made("4093.9", "1", "45");
    let center = at("0", "0");
    let radius = Num::int(2048);
    for probe in [
        // 1 448·√2 = 2 047.78 m across the long edge's normal, 0.72 m into the body's reach.
        at("-1448", "-1448"),
        // 1.5·√½ = 1.06 m across it, so 0.56 m past the line and 2 047.44 m in.
        at("1000", "-1001.5"),
        // 2 895·√2 = 4 094.15 m along the axis, 2 047.20 m past the short edge, 0.80 m in.
        at("2895", "-2895"),
        // 2 047.50 m along −z from the corner, between the long and short edges' normals.
        at("1447.06", "-3495.27"),
    ] {
        assert!(
            longest.push_out(center, probe, radius).is_some(),
            "{probe:?}"
        );
        check_push(&longest, center, probe, radius);
    }
}
