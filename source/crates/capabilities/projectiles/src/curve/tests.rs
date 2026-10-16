use super::*;

/// A curve's spec of inner points at `first` and `second`, `[fraction, height]` each, with no
/// follow and no life.
fn spec(first: [Num; 2], second: [Num; 2]) -> CurveSpec {
    CurveSpec {
        first,
        second,
        follow: Num::ZERO,
        life: None,
    }
}

fn quarter(quarters: i64) -> Num {
    Num::QUARTER * quarters
}

#[test]
fn a_straight_curve_has_the_points_of_its_line_and_its_chord_for_its_length() {
    // From the origin to 32 m along x, inner points at a quarter and three quarters with no
    // height: 8 and 24 m. Its polygon is its chord, 32 m, so its length is 32 and, at 7 m a tick,
    // its count ⌈32 ÷ 7⌉ = 5. At t = k ÷ 4, x is 3(1 − t)²t · 8 + 3(1 − t)t² · 24 + t³ · 32: 0,
    // 27/8 + 27/8 + 1/2 = 7.25, 3 + 9 + 4 = 16, 24.75 and 32.
    let line = Curve::new(
        Vec3::ZERO,
        Vec3::from((32, 0, 0)),
        &spec([quarter(1), Num::ZERO], [quarter(3), Num::ZERO]),
        None,
    );
    assert_eq!(line.length(), Num::int(32));
    assert_eq!(line.count(Num::int(7)), 5);
    let xs = [0, 1, 2, 3, 4].map(|k| line.point(k, 5));
    let expected =
        [0, 29, 64, 99, 128].map(|quarters| Vec3::new(quarter(quarters), Num::ZERO, Num::ZERO));
    assert_eq!(xs, expected);
    // One point is the first; a curve of no length has one; a speed of a bit gives the most.
    assert_eq!(line.point(0, 1), Vec3::ZERO);
    let still = Curve::new(
        Vec3::ZERO,
        Vec3::ZERO,
        &spec([Num::HALF, Num::ZERO], [Num::HALF, Num::ZERO]),
        None,
    );
    assert_eq!(still.length(), Num::ZERO);
    assert_eq!(still.count(Num::ONE), 1);
    assert_eq!(line.count(Num::EPSILON), MAX_POINTS);
}

#[test]
fn a_curves_points_are_the_bezier_formula_over_the_highest_of_its_ends_and_ground() {
    // From the origin to (8, 0, 0), its inner points 4 m up at a quarter and three quarters:
    // (2, 4, 0) and (6, 4, 0). At t = ½, x is 3 · ¼ · ½ · 2 + 3 · ½ · ¼ · 6 + ⅛ · 8 = 4, and y is
    // 12t(1 − t) = 3.
    let shape = spec([quarter(1), Num::int(4)], [quarter(3), Num::int(4)]);
    let arc = Curve::new(Vec3::ZERO, Vec3::from((8, 0, 0)), &shape, None);
    let points = [0, 1, 2].map(|k| arc.point(k, 3));
    assert_eq!(
        points,
        [Vec3::ZERO, Vec3::from((4, 3, 0)), Vec3::from((8, 0, 0))]
    );
    // Its inner points rise above the highest of its ends and the ground: an end 2 m up, or a
    // ground 5 m up between them, lifts them to 6 or 9 m, and at t = ½ the end's own height adds
    // t³ · 2 = ¼.
    let raised = Curve::new(Vec3::ZERO, Vec3::from((8, 2, 0)), &shape, None);
    assert_eq!(raised.point(1, 3).y, Num::int(9) / 2 + quarter(1));
    let hill = Curve::new(Vec3::ZERO, Vec3::from((8, 0, 0)), &shape, Some(Num::int(5)));
    assert_eq!(hill.point(1, 3).y, Num::int(9) * 3 / 4);
    let low = Curve::new(
        Vec3::ZERO,
        Vec3::from((8, 0, 0)),
        &shape,
        Some(-Num::int(5)),
    );
    assert_eq!(low.point(1, 3), arc.point(1, 3));
}

#[test]
fn a_curves_length_splits_at_its_middle_while_its_polygon_passes_its_chord_by_a_meter() {
    // The arc above: its chord 8, its polygon √20 + 4 + √20 ≈ 12.94, past it by more than 1 m,
    // so it splits. Its left half, (0, 0), (1, 2), (2.5, 3), (4, 3), has the chord 5 and the
    // polygon √5 + √3.25 + 1.5 ≈ 5.54, within 1 m of it, and its right half mirrors it: the
    // length is twice (5 + √5 + √3.25 + 1.5) ÷ 2, each root to the nearest bit.
    let shape = spec([quarter(1), Num::int(4)], [quarter(3), Num::int(4)]);
    let arc = Curve::new(Vec3::ZERO, Vec3::from((8, 0, 0)), &shape, None);
    let root = |x: Num, y: Num| Vec3::new(x, y, Num::ZERO).length();
    let half =
        (Num::int(5) + root(Num::ONE, Num::int(2)) + root(quarter(6), Num::ONE) + quarter(6)) / 2;
    assert_eq!(arc.length(), half * 2);
    // Unsplit, its length would be (8 + 12.94) ÷ 2, shorter.
    let unsplit = (Num::int(8) + root(Num::int(2), Num::int(4)) * 2 + Num::int(4)) / 2;
    assert!(unsplit < arc.length(), "{unsplit} against {}", arc.length());
}
