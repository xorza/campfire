use super::*;
use crate::geometry::metric::Approach;

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
    // A diagonal of 126 m holds a 126 m side only with no other; one of 125.9 and 1 m is
    // within it, 62.95² + 0.5² = 3 963.0525 ≤ 63², and at 45° its corners stay within 64 m.
    assert!(BodyBox::new([num("126"), least], Num::ZERO).is_none());
    assert!(BodyBox::new([num("126"), Num::ZERO], Num::ZERO).is_none());
    let longest = made("125.9", "1", "45");
    let [a, b] = longest.frame().halves;
    let reach = (64 * i128::from(Num::ONE.to_bits())).pow(2);
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
    // clockwise, lie flat, or reach past 64 m do not decode.
    let decoded: BodyBox = postcard::from_bytes(&postcard::to_allocvec(&body).unwrap()).unwrap();
    assert_eq!(decoded, body);
    let encode =
        |half: [[&str; 2]; 2]| postcard::to_allocvec(&half.map(|edge| edge.map(num))).unwrap();
    for half in [
        [["0", "1"], ["2", "0"]],
        [["2", "0"], ["4", "0"]],
        [["50", "0"], ["0", "40"]],
    ] {
        assert!(
            postcard::from_bytes::<BodyBox>(&encode(half)).is_err(),
            "{half:?}"
        );
    }
    // A component as large as a number holds is refused, with no product of it.
    let huge = postcard::to_allocvec(&[[Num::MAX, Num::ZERO], [Num::ZERO, Num::MAX]]).unwrap();
    assert!(postcard::from_bytes::<BodyBox>(&huge).is_err());
    assert!(postcard::from_bytes::<BodyBox>(&encode([["2", "0"], ["0", "1"]])).is_ok());
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
        BoxApproach {
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
        BoxApproach {
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
        BoxApproach {
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
        BoxApproach {
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
        BoxApproach {
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
    // 11 863 283.2 bits along each axis, to the nearest bit.
    let off_corner = body
        .push_out(centre, point("2.5", "1.5"), Num::ONE)
        .unwrap();
    assert_eq!(off_corner.x.to_bits(), 45_417_715);
    assert_eq!(off_corner.z.to_bits(), 28_640_499);
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

/// Draws for the random boxes: sizes, angles, offsets and reaches, every run the same.
struct Draws {
    next: Box<dyn FnMut() -> u64>,
}

impl Draws {
    const ONE: i64 = Num::ONE.to_bits();

    fn bits(&mut self, low: i64, high: i64) -> i64 {
        let span = u64::try_from(high - low).unwrap();
        low + i64::try_from((self.next)() % span).unwrap()
    }

    fn size(&mut self) -> Num {
        Num::from_bits(self.bits(BodyBox::MIN_SIZE.to_bits(), 20 * Draws::ONE))
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
        let mut axis = || Num::from_bits(self.bits(-span * Draws::ONE, span * Draws::ONE));
        let offset = Vec3::new(axis(), Num::ZERO, axis());
        Position::new(from.get() + offset).unwrap()
    }

    /// A reach below `meters`.
    fn reach(&mut self, meters: i64) -> Num {
        Num::from_bits(self.bits(0, meters * Draws::ONE))
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

/// A body pushed out ends touching the box, within the rounding of its end, a bit and a half,
/// and a bit more; one left alone did not overlap it.
fn check_push(body: &BodyBox, centre: Position, probe: Position, radius: Num) {
    let two_bits = Num::from_bits(2);
    match body.push_out(centre, probe.get(), radius) {
        Some(moved) => {
            let moved = Position::new(moved).unwrap();
            assert_ne!(
                body.nearest(centre, moved, radius + two_bits),
                Ordering::Greater
            );
            assert_ne!(
                body.nearest(centre, moved, radius - two_bits),
                Ordering::Less
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
    let mut draws = Draws {
        next: Box::new(split_mix(0xB0C5)),
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
        let radius = Num::from_bits(draws.bits(BodyBox::MIN_SIZE.to_bits(), 3 * Draws::ONE));
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
