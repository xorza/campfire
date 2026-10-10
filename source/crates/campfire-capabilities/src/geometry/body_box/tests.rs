use campfire_common::Binary;

use super::*;

impl BodyBox {
    /// Whether `at` lies inside the box at `centre` or on its edge.
    fn holds(&self, centre: Position, at: Position) -> bool {
        self.frame().holds(sub(flat(at), flat(centre)))
    }
}

fn num(text: &str) -> Num {
    text.parse().unwrap()
}

fn at(x: &str, z: &str) -> Position {
    Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap()
}

fn made(width: &str, height: &str, angle: &str) -> BodyBox {
    BodyBox::new([num(width), num(height)], num(angle)).unwrap()
}

fn halves(body: &BodyBox) -> [[i64; 2]; 2] {
    body.half_edges().map(|edge| edge.map(Num::to_bits))
}

/// The box's corners at `centre`, as a polygon.
fn polygon(body: &BodyBox, centre: Position) -> Polygon {
    let at = flat(centre);
    let points = body
        .frame()
        .corners()
        .map(|corner| add(corner, at).map(|bits| Num::from_bits(i64::try_from(bits).unwrap())));
    Polygon::new(points.to_vec()).unwrap()
}

fn flat_vec3(off: Flat) -> Vec3 {
    let num = |bits: i128| Num::from_bits(i64::try_from(bits).unwrap());
    Vec3::new(num(off[0]), Num::ZERO, num(off[1]))
}

/// Where the path from `start` along `path` crosses the segment from `a` to `b`, as a share of
/// the path, by the two lines' cross products; none for parallel lines or no crossing.
fn crossing(start: Flat, path: Flat, a: Flat, b: Flat) -> Option<Fraction> {
    let edge = sub(b, a);
    let den = cross(path, edge);
    if den == 0 {
        return None;
    }
    let to_a = sub(a, start);
    let (t, u) = (cross(to_a, edge), cross(to_a, path));
    let within = |num: i128| {
        let fraction = Fraction::new(num, den);
        Fraction::ZERO <= fraction && fraction <= Fraction::ONE
    };
    (within(t) && within(u)).then(|| Fraction::new(t, den))
}

/// `SplitMix64` from `seed`: draws that every run repeats.
fn split_mix(seed: u64) -> impl FnMut() -> u64 {
    let mut state = seed;
    move || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[test]
fn a_box_turns_exactly_by_quarters_and_rounds_once_elsewhere() {
    let one = Num::ONE.to_bits();
    // 4 × 2 m: half edges (2, 0) and (0, 1), turned a quarter at a time, and the same a whole
    // turn on, or before.
    let quarters = [
        ("0", [[2, 0], [0, 1]]),
        ("90", [[0, 2], [-1, 0]]),
        ("180", [[-2, 0], [0, -1]]),
        ("270", [[0, -2], [1, 0]]),
        ("360", [[2, 0], [0, 1]]),
        ("-90", [[0, -2], [1, 0]]),
        ("450", [[0, 2], [-1, 0]]),
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
    // At 45°, a 2 m side's half edge is the cosine and the sine themselves: within a bit of
    // cos 45° × 2²⁴ = 11 863 283.2, as `sin_cos` is within 0.501 of a bit of the sine of the
    // rounded angle, itself within 2⁻²⁵ rad of π/4. The other half edge is the first turned a
    // quarter, from the same products.
    let turned = halves(&made("2", "2", "45"));
    for component in turned[0] {
        assert!((component - 11_863_283).abs() <= 1, "{component}");
    }
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
    assert!(dot(add(a, b), add(a, b)) <= reach && dot(sub(a, b), sub(a, b)) <= reach);
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
    let centre = at("10", "5");
    // A bit is 2⁻²⁴ m, 0.000000059604644775390625.
    for (x, z, inside) in [
        ("10", "5", true),
        ("12", "6", true),
        ("8", "4", true),
        ("11", "6", true),
        ("12", "6.000000059604644775390625", false),
        ("12.000000059604644775390625", "5", false),
    ] {
        assert_eq!(body.holds(centre, at(x, z)), inside, "({x}, {z})");
    }
    // Turned a quarter, it reaches 1 m along x and 2 m along z.
    let upright = made("4", "2", "90");
    assert!(upright.holds(centre, at("11", "7")));
    assert!(!upright.holds(centre, at("12", "6")));
}

#[test]
fn a_distance_runs_to_the_nearest_edge_or_corner() {
    let body = made("4", "2", "0");
    let centre = at("0", "0");
    let bit = Num::EPSILON;
    // 3 m from the edge at x = 2: within a reach of 3, not of 3 less a bit.
    assert_eq!(
        body.nearest(centre, at("5", "0"), num("3")),
        Ordering::Equal
    );
    assert_eq!(
        body.nearest(centre, at("5", "0"), num("3") - bit),
        Ordering::Greater
    );
    // From the corner (2, 1) to (5, 4), √18 m: past 4, within 5.
    assert_eq!(
        body.nearest(centre, at("5", "4"), num("4")),
        Ordering::Greater
    );
    assert_eq!(body.nearest(centre, at("5", "4"), num("5")), Ordering::Less);
    // Inside, 0: a reach of 0 meets it.
    assert_eq!(
        body.nearest(centre, at("1", "0.5"), Num::ZERO),
        Ordering::Equal
    );
    // Two distances compare exactly: 3 m from the edge at x = 2 and from the one at z = 1;
    // a bit further from the second is further.
    let side = body.distance(centre, at("5", "0"));
    assert_eq!(side, body.distance(centre, at("0", "4")));
    assert!(body.distance(centre, at("0", "4.000000059604644775390625")) > side);
    // √18 from the corner against 4 from an edge: 18 > 16.
    assert!(body.distance(centre, at("5", "4")) > body.distance(centre, at("0", "5")));
    // Turned a quarter, the edge at x = 1 lies 3 m from (4, 0).
    let upright = made("4", "2", "90");
    assert_eq!(
        upright.nearest(centre, at("4", "0"), num("3")),
        Ordering::Equal
    );
}

#[test]
fn the_nearest_point_lies_on_the_nearest_edge_or_corner() {
    let body = made("4", "2", "0");
    let centre = at("0", "0");
    assert_eq!(body.nearest_point(centre, at("5", "0")), at("2", "0"));
    assert_eq!(body.nearest_point(centre, at("5", "4")), at("2", "1"));
    assert_eq!(body.nearest_point(centre, at("1", "0.5")), at("1", "0.5"));
    // Turned 45°, the point nearest one beside its edge lies on that edge, within a bit, and as
    // far from it as the box is, within the bit it rounds by.
    let turned = made("4", "2", "45");
    let probe = at("3", "-3");
    let point = turned.nearest_point(centre, probe);
    assert_eq!(turned.nearest(centre, point, Num::EPSILON), Ordering::Less);
    // Each coordinate rounds by half a bit at most, so the way to the point is within a bit of
    // the exact distance d, and its square within 2d + 1 of d².
    let way = dot(sub(flat(point), flat(probe)), sub(flat(point), flat(probe))).unsigned_abs();
    let slack = 2 * (way.floor_root() + 1) + 1;
    let gap = turned.distance(centre, probe);
    assert!(SquaredDistance::whole(way - slack) < gap && gap < SquaredDistance::whole(way + slack));
}

#[test]
fn twice_the_scale_sees_halves_of_a_bit() {
    let body = made("2", "2", "0");
    let twice = |meters: i128, halves: i128| (meters << 25) + halves;
    // 1.5 m along x lies half a meter off the edge: not closer than half a meter, closer than a
    // bit more.
    let off = [twice(1, 1 << 24), 0];
    assert!(!body.closer_twice(off, Num::HALF));
    assert!(body.closer_twice(off, Num::HALF + Num::EPSILON));
    // Half a bit past the edge: closer than a bit, not than nothing.
    let off = [twice(1, 1), 0];
    assert!(body.closer_twice(off, Num::EPSILON));
    assert!(!body.closer_twice(off, Num::ZERO));
    // A square of 1 m centred 1.5 m along x touches the box's edge; half a bit nearer, it
    // overlaps.
    let half = 1 << 24;
    assert!(!body.overlaps_square_twice([twice(1, 1 << 24), 0], half));
    assert!(body.overlaps_square_twice([twice(1, (1 << 24) - 1), 0], half));
}

#[test]
fn a_path_comes_nearest_where_it_enters_or_at_its_first_nearest_point() {
    let body = made("4", "2", "0");
    let centre = at("0", "0");
    let share = |num: i128, den: i128| Fraction::new(num, den);
    let approach =
        |from: Position, to: Position, reach: &str| body.approach(centre, from, to, num(reach));
    // Through it along x from -10: it enters at x = -2, 8 of the path's 20 m; from the other
    // end, at x = 2, 8 m in too.
    let through = approach(at("-10", "0"), at("10", "0"), "0");
    assert_eq!(
        through,
        Approach {
            nearest: Ordering::Equal,
            share: share(2, 5)
        }
    );
    assert_eq!(
        approach(at("-10", "0"), at("10", "0"), "1").nearest,
        Ordering::Less
    );
    assert_eq!(
        approach(at("10", "0"), at("-10", "0"), "0").share,
        share(2, 5)
    );
    // From inside: at its start.
    assert_eq!(
        approach(at("0", "0"), at("10", "0"), "0").share,
        Fraction::ZERO
    );
    // Along z = 3, 2 m off the edge at z = 1 from x = -2 to 2: the first of that stretch.
    let beside = approach(at("-10", "3"), at("10", "3"), "2");
    assert_eq!(
        beside,
        Approach {
            nearest: Ordering::Equal,
            share: share(2, 5)
        }
    );
    let short = body.approach(
        centre,
        at("-10", "3"),
        at("10", "3"),
        num("2") - Num::EPSILON,
    );
    assert_eq!(short.nearest, Ordering::Greater);
    // From (5, 5) to (10, 0), the corner (2, 1) is nearest a tenth of the way, 7/√2 m off,
    // 24.5 m²: within 5, past 4.9, 24.01 m².
    let corner = approach(at("5", "5"), at("10", "0"), "5");
    assert_eq!(
        corner,
        Approach {
            nearest: Ordering::Less,
            share: share(1, 10)
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
            share: Fraction::ZERO
        }
    );
    // Past its end: the end is nearest, 1 m short of the edge.
    let short_of = approach(at("-10", "0"), at("3", "0"), "1");
    assert_eq!(short_of.share, share(8, 13));
    let stops = approach(at("10", "0"), at("3", "0"), "1");
    assert_eq!(
        stops,
        Approach {
            nearest: Ordering::Equal,
            share: Fraction::ONE
        }
    );
}

#[test]
fn a_body_is_pushed_out_of_a_box() {
    let body = made("4", "2", "0");
    let centre = at("0", "0");
    let point = |x: &str, z: &str| at(x, z).get();
    // Half a meter into the edge at x = 2, from outside and from inside: out to touch it.
    assert_eq!(
        body.push_out(centre, point("2.5", "0"), Num::ONE),
        Some(point("3", "0"))
    );
    assert_eq!(
        body.push_out(centre, point("1.5", "0"), Num::ONE),
        Some(point("3", "0"))
    );
    // Off the corner (2, 1) by (0.5, 0.5): out to 1 m from it along the diagonal, 2²⁴/√2 =
    // 11 863 283.2 bits along each axis, rounded up to 11 863 284, so it ends 16 777 217.1
    // bits from the corner, past its radius of 2²⁴, and a second push finds no overlap.
    let off_corner = body
        .push_out(centre, point("2.5", "1.5"), Num::ONE)
        .unwrap();
    assert_eq!(off_corner.x.to_bits(), 45_417_716);
    assert_eq!(off_corner.z.to_bits(), 28_640_500);
    assert_eq!(body.push_out(centre, off_corner, Num::ONE), None);
    // At the centre of a square, every edge is as near: out through the first, at z = -1.
    let square = made("2", "2", "0");
    assert_eq!(
        square.push_out(centre, point("0", "0"), Num::ONE),
        Some(point("0", "-2"))
    );
    // Clear of it, and touching it, no push.
    assert_eq!(body.push_out(centre, point("3.5", "0"), Num::ONE), None);
    assert_eq!(body.push_out(centre, point("3", "0"), Num::ONE), None);
    // Turned a quarter, its edge at z = 2.
    let upright = made("4", "2", "90");
    assert_eq!(
        upright.push_out(centre, point("0", "2.5"), Num::ONE),
        Some(point("0", "3"))
    );
    // Turned 30°, its half edges are a = (29 058 990, 16 777 216) and b = (−8 388 608,
    // 14 529 495) bits, and a body of 1 m moves along a normal `n` of the edge or the way from
    // the corner, by `n · (2²⁴·√|n|² − out) ÷ |n|²`, where `off` lies `out ÷ √|n|²` beyond the
    // edge or the corner, each coordinate rounded away from the box.
    let turned = made("4", "2", "30");
    let bits = |x: i64, z: i64| Vec3::new(Num::from_bits(x), Num::ZERO, Num::from_bits(z));
    let cases = [
        // Off the edge from a to a + b, 1.25·a: n = a, out = a · 0.25·a = 281 474 983 662 184,
        // a move of (7 264 747.32, 4 194 303.90), rounded (7 264 748, 4 194 304).
        (bits(36_323_738, 20_971_520), bits(43_588_486, 25_165_824)),
        // Off the corner a + b = (20 670 382, 31 306 711) by (0.25, 0.25) m: n = (2²², 2²²),
        // out = 2⁴⁵, a move of 2²⁴/√2 − 2²² = 7 668 979.20 along each axis, rounded 7 668 980.
        (bits(24_864_686, 35_501_015), bits(32_533_666, 43_169_995)),
        // Inside at (1, 0) m, 0.5 m from the edge from −a − b to a − b, the nearest of 0.5,
        // 1.13, 1.5 and 2.87 m: n = (2²⁵, −58 117 980), out = −562 949 923 109 444, a move of
        // (12 582 911.999 999 999 2, −21 794 242.499 999 998 7), rounded (12 582 912,
        // −21 794 243).
        (bits(16_777_216, 0), bits(29_360_128, -21_794_243)),
    ];
    for (from, to) in cases {
        let moved = turned.push_out(centre, from, Num::ONE);
        assert_eq!(moved, Some(to), "{from:?}");
        assert_eq!(turned.push_out(centre, to, Num::ONE), None, "{from:?}");
    }
    // The height stays.
    let high = Vec3::new(num("2.5"), num("7"), Num::ZERO);
    assert_eq!(body.push_out(centre, high, Num::ONE).unwrap().y, num("7"));
}

#[test]
fn boxes_overlap_when_their_insides_meet() {
    let body = made("2", "2", "0");
    let origin = at("0", "0");
    let bit = Num::EPSILON;
    let beside = |x: Num| Position::new(Vec3::new(x, Num::ZERO, Num::ZERO)).unwrap();
    // Side by side, they touch and do not overlap; a bit closer, they do.
    assert!(!body.overlaps(origin, &body, beside(num("2"))));
    assert!(body.overlaps(origin, &body, beside(num("2") - bit)));
    assert_eq!(
        body.nearest_box(origin, &body, beside(num("2")), Num::ZERO),
        Ordering::Equal
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
        Polygon::new(points.iter().map(|&(x, z)| [num(x), num(z)]).collect()).unwrap()
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
    next: Box<dyn FnMut() -> u64>,
    scale: i64,
}

impl Draws {
    const ONE: i64 = Num::ONE.to_bits();

    /// A meter, scaled.
    const fn meter(&self) -> i64 {
        Draws::ONE * self.scale
    }

    fn bits(&mut self, low: i64, high: i64) -> i64 {
        let span = u64::try_from(high - low).unwrap();
        low + i64::try_from((self.next)() % span).unwrap()
    }

    fn size(&mut self) -> Num {
        let high = 20 * self.meter();
        Num::from_bits(self.bits(BodyBox::MIN_SIZE.to_bits(), high))
    }

    /// A whole number of quarters, or any angle.
    fn angle(&mut self, quarters: bool) -> Num {
        if quarters {
            Num::from_bits(self.bits(0, 4) * 90 * Draws::ONE)
        } else {
            Num::from_bits(self.bits(0, 360 * Draws::ONE))
        }
    }

    fn body(&mut self, quarters: bool) -> BodyBox {
        let size = [self.size(), self.size()];
        BodyBox::new(size, self.angle(quarters)).unwrap()
    }

    /// A point within `span` meters of `from` along each axis.
    fn near(&mut self, from: Position, span: i64) -> Position {
        let meter = self.meter();
        let mut axis = || Num::from_bits(self.bits(-span * meter, span * meter));
        let offset = Vec3::new(axis(), Num::ZERO, axis());
        Position::new(from.get() + offset).unwrap()
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
        .map(|(path, off)| Approach::of(flat_vec3(path), flat_vec3(off), reach).nearest)
        .min()
        .unwrap()
}

/// The approaches of `point`, from the box's position, to each of the box's edges.
fn to_edges(body: &BodyBox, point: Flat) -> impl Iterator<Item = (Flat, Flat)> {
    let corners = body.frame().corners();
    (0..4).map(move |edge| {
        let start = corners[edge];
        (sub(corners[(edge + 1) % 4], start), sub(point, start))
    })
}

/// A point's distance against a reach agrees with its approaches to the edges, and whether
/// the box holds it with the polygon of its corners.
fn check_point(body: &BodyBox, centre: Position, probe: Position, reach: Num) {
    let at = probe.get();
    let holds = body.holds(centre, probe);
    assert_eq!(
        holds,
        polygon(body, centre).holds_point([at.x, at.z]),
        "{body:?} {probe:?}"
    );
    let expected = if holds {
        Num::ZERO.cmp(&reach)
    } else {
        by_approaches(to_edges(body, sub(flat(probe), flat(centre))), reach)
    };
    assert_eq!(
        body.nearest(centre, probe, reach),
        expected,
        "{body:?} {probe:?}"
    );
}

/// A path's approach agrees with its crossings of the edges where it meets the box, and else
/// with the approaches of its ends to the edges and of the corners to it.
fn check_path(body: &BodyBox, centre: Position, from: Position, to: Position, reach: Num) {
    let approach = body.approach(centre, from, to, reach);
    let (start, path) = (sub(flat(from), flat(centre)), sub(flat(to), flat(from)));
    let corners = body.frame().corners();
    let enters = (0..4)
        .filter_map(|edge| crossing(start, path, corners[edge], corners[(edge + 1) % 4]))
        .min();
    let enters = if body.holds(centre, from) {
        Some(Fraction::ZERO)
    } else {
        enters
    };
    if let Some(enters) = enters {
        assert_eq!(approach.share, enters, "{body:?} {from:?} {to:?}");
        assert_eq!(approach.nearest, Num::ZERO.cmp(&reach));
        return;
    }
    let ends = to_edges(body, start).chain(to_edges(body, add(start, path)));
    let tips = corners.into_iter().map(|corner| (path, sub(corner, start)));
    let expected = by_approaches(ends.chain(tips), reach);
    assert_eq!(
        approach.nearest, expected,
        "{body:?} {from:?} {to:?} {reach:?}"
    );
}

/// A body pushed out ends touching the box or apart, so a second push finds no overlap, and
/// less than 2 bits past touching, as each coordinate rounds outward by less than a bit; one
/// left alone did not overlap it.
fn check_push(body: &BodyBox, centre: Position, probe: Position, radius: Num) {
    let two_bits = Num::from_bits(2);
    match body.push_out(centre, probe.get(), radius) {
        Some(moved) => {
            let case = format!("{body:?} {centre:?} {probe:?} {radius:?}");
            assert_eq!(body.push_out(centre, moved, radius), None, "{case}");
            let moved = Position::new(moved).unwrap();
            assert_ne!(
                body.nearest(centre, moved, radius),
                Ordering::Less,
                "{case}"
            );
            assert_eq!(
                body.nearest(centre, moved, radius + two_bits),
                Ordering::Less,
                "{case}"
            );
        }
        None => assert_ne!(body.nearest(centre, probe, radius), Ordering::Less),
    }
}

/// Two boxes' overlap agrees both ways round and with one box and the other's polygon, and
/// their distance against a reach with each corner's approaches to the other's edges. Whether
/// they overlap.
fn check_pair(body: &BodyBox, centre: Position, other: &BodyBox, at: Position, reach: Num) -> bool {
    let overlaps = body.overlaps(centre, other, at);
    assert_eq!(overlaps, other.overlaps(at, body, centre));
    let by_polygon = body.overlaps_polygon(centre, &polygon(other, at));
    assert_eq!(overlaps, by_polygon, "{body:?} {centre:?} {other:?} {at:?}");
    let expected = if overlaps {
        Num::ZERO.cmp(&reach)
    } else {
        let apart = sub(flat(at), flat(centre));
        let ours = body
            .frame()
            .corners()
            .into_iter()
            .flat_map(|corner| to_edges(other, sub(corner, apart)));
        let theirs = other
            .frame()
            .corners()
            .into_iter()
            .flat_map(|corner| to_edges(body, add(corner, apart)));
        by_approaches(ours.chain(theirs), reach)
    };
    assert_eq!(body.nearest_box(centre, other, at, reach), expected);
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
        next: Box::new(split_mix(0xB0C5)),
        scale,
    };
    let origin = Position::new(Vec3::ZERO).unwrap();
    let (mut overlapping, mut apart) = (0, 0);
    for round in 0..400 {
        let quarters = round % 4 == 0;
        let body = draws.body(quarters);
        let centre = draws.near(origin, 50);
        for _ in 0..8 {
            let (probe, reach) = (draws.near(centre, 15), draws.reach(10));
            check_point(&body, centre, probe, reach);
        }
        let (from, to, reach) = (
            draws.near(centre, 20),
            draws.near(centre, 20),
            draws.reach(5),
        );
        check_path(&body, centre, from, to, reach);
        let high = 3 * draws.meter();
        let radius = Num::from_bits(draws.bits(BodyBox::MIN_SIZE.to_bits(), high));
        check_push(&body, centre, draws.near(centre, 12), radius);
        let other = draws.body(quarters);
        let (at, reach) = (draws.near(centre, 25), draws.reach(10));
        if check_pair(&body, centre, &other, at, reach) {
            overlapping += 1;
        } else {
            apart += 1;
        }
    }
    // Both outcomes came up often enough to test each.
    assert!(overlapping > 50 && apart > 50, "{overlapping} {apart}");
}

/// Whether `k` moves a body of `radius` out along a normal of square `square` whose component
/// is `along`, from `out` beyond its line: `k · square + along · out ≥ along · radius · √square`.
fn moves_out(k: i128, along: i128, out: i128, square: i128, radius: i128) -> bool {
    let side = k * square + along * out;
    let product = (along * radius).unsigned_abs();
    let bound = U256::product(product, product)
        .checked_mul(square.unsigned_abs())
        .unwrap();
    side >= 0 && U256::product(side.unsigned_abs(), side.unsigned_abs()) >= bound
}

#[test]
fn a_push_is_the_least_whole_move_and_its_first_try_one_below_it_at_most() {
    // Every small case, against a scan from 0: the least `k` that moves the body out.
    for square in 1..40_i128 {
        for along in (0..=square).filter(|along| along * along <= square) {
            for radius in 1..12_i128 {
                for out in -60..60_i128 {
                    if radius * radius * square <= out * out.abs() {
                        continue;
                    }
                    // k · square ≥ along · (radius · square + |out|) passes, as square ≥ √square.
                    let most = along * (radius * square + out.abs());
                    let least = (0..=most)
                        .find(|&k| moves_out(k, along, out, square, radius))
                        .unwrap();
                    let case = format!("{along} {out} {square} {radius}");
                    assert_eq!(outward(along, out, square, radius), least, "{case}");
                    let first = first_outward(along, out, square, radius);
                    assert!(first == least || first + 1 == least, "{case}");
                }
            }
        }
    }
    // At the bound: a body of 2,048 m, 2³⁵ bits, against an edge just short of 4,094 m, along an
    // axis, `along` its length, and at 45°, `along` its length ÷ √2: from inside, an edge's length
    // behind the line, `out` = −square; from the line; from near the body's reach, the length
    // times 2³⁵ − 1 bits; and between. Then off a corner by the least way, 1 bit, its square 1, to
    // 2³⁵ − 1 bits.
    let radius = 1_i128 << 35;
    let meter = i128::from(Num::ONE.to_bits());
    let (axis, aslant) = (4093 * meter, 2894 * meter);
    for (along, square) in [(axis, axis * axis), (aslant, 2 * aslant * aslant)] {
        let length = square.cast_unsigned().floor_root().cast_signed();
        for out in [
            -square,
            0,
            length * (radius - 1),
            length * (radius >> 1) + 12_345,
        ] {
            let k = outward(along, out, square, radius);
            let case = format!("{along} {out}");
            assert!(moves_out(k, along, out, square, radius), "{case}");
            assert!(!moves_out(k - 1, along, out, square, radius), "{case}");
            let first = first_outward(along, out, square, radius);
            assert!(first == k || first + 1 == k, "{case}");
        }
    }
    assert_eq!(outward(1, 1, 1, radius), radius - 1);
}

#[test]
fn a_body_at_the_bound_is_pushed_out_of_the_longest_box() {
    // The longest box, turned 45°, and a body of 2,048 m into it: each push ends touching or
    // apart, under 2 bits past touching. The box's half edges are 2 046.95 m along (√½, √½) and
    // 0.5 m along (−√½, √½), so its long edge's line lies 0.5 m from the centre, its short edge's
    // 2 046.95 m, and a corner at (1 447.06, 1 447.77) m: 1 447.41 ∓ 0.35, 1 447.41 + 0.35.
    let longest = made("4093.9", "1", "45");
    let centre = at("0", "0");
    let radius = Num::int(2048);
    for probe in [
        // 1 448·√2 = 2 047.78 m across the long edge's normal, 0.72 m into the body's reach.
        at("-1448", "1448"),
        // 1.5·√½ = 1.06 m across it, so 0.56 m past the line and 2 047.44 m in.
        at("1000", "1001.5"),
        // 2 895·√2 = 4 094.15 m along the axis, 2 047.20 m past the short edge, 0.80 m in.
        at("2895", "2895"),
        // 2 047.50 m straight up from the corner, between the long and short edges' normals.
        at("1447.06", "3495.27"),
    ] {
        assert!(
            longest.push_out(centre, probe.get(), radius).is_some(),
            "{probe:?}"
        );
        check_push(&longest, centre, probe, radius);
    }
}
