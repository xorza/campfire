use proptest::prelude::*;

use super::*;

const CASES: u32 = 10_000;

/// Whether each axis of `scaled`, `length` along `vector`, lies from its true value to less than
/// a unit past it, away from zero; the reference lies within 2⁻¹⁰ of a unit of the true one.
#[expect(
    clippy::float_arithmetic,
    clippy::cast_precision_loss,
    reason = "f64 is only the accuracy reference"
)]
fn scaled_within<const N: usize>(vector: [i128; N], length: i128, scaled: [i128; N]) -> bool {
    let norm = vector
        .iter()
        .map(|&axis| (axis * axis) as f64)
        .sum::<f64>()
        .sqrt();
    vector.iter().zip(scaled).all(|(&axis, scaled)| {
        let exact = (axis as f64 * length as f64 / norm).abs();
        let past = scaled.abs() as f64 - exact;
        scaled.signum() * axis.signum() >= 0 && past > -1e-3 && past < 1.0 + 1e-3
    })
}

/// Each measure's distance from the true one: the length, how far along, and how far apart. The
/// dot and the cross products are exact, below 2⁸³ for axes below 2⁴¹; their floats, and the
/// root, lie within 2⁻⁵² of themselves, so each reference lies within 2⁻¹⁰ of a unit of the true
/// value.
#[expect(
    clippy::float_arithmetic,
    clippy::cast_precision_loss,
    reason = "f64 is only the accuracy reference"
)]
fn errors<const N: usize>(
    direction: &Direction<N>,
    vector: [i128; N],
    off: [i128; N],
    apart: i128,
) -> [f64; 3] {
    let measured = [direction.length(), direction.along(off), apart].map(|value| value as f64);
    let (v, o) = (&vector[..], &off[..]);
    let dot: i128 = v.iter().zip(o).map(|(a, b)| a * b).sum();
    let cross = if N == 2 {
        vec![v[0] * o[1] - v[1] * o[0]]
    } else {
        vec![
            v[1] * o[2] - v[2] * o[1],
            v[2] * o[0] - v[0] * o[2],
            v[0] * o[1] - v[1] * o[0],
        ]
    };
    let length = v
        .iter()
        .map(|&axis| (axis * axis) as f64)
        .sum::<f64>()
        .sqrt();
    let cross = cross
        .iter()
        .map(|&axis| (axis as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    let exact = [length, dot as f64 / length, cross / length];
    [0, 1, 2].map(|at| (measured[at] - exact[at]).abs())
}

#[test]
fn a_direction_measures_by_hand() {
    // (3, 4) is 5 long, its unit (0.6, 0.8): (3, 4) lies 5 along it and (4, −3) 5 apart, and
    // behind it, −5 along.
    let tilted = Direction::of([3, 4]).unwrap();
    assert_eq!(tilted.length(), 5);
    assert_eq!(tilted.along([3, 4]), 5);
    assert_eq!(tilted.along([-3, -4]), -5);
    assert_eq!(tilted.apart([4, -3]), 5);
    assert_eq!(tilted.apart([3, 4]), 0);
    // Along an axis each measure is exact: (a, b) lies a along (7, 0) and |b| apart.
    let axis = Direction::of([7, 0]).unwrap();
    assert_eq!((axis.along([-9, 4]), axis.apart([-9, 4])), (-9, 4));
    // Each rounds to the nearest unit: (1, 0) lies √½ along (1, 1) and √½ apart, so 1 and 1;
    // (1, 1) is √2 long, so 1, and (1, 2) √5, so 2.
    let diagonal = Direction::of([1, 1]).unwrap();
    assert_eq!((diagonal.along([1, 0]), diagonal.apart([1, 0])), (1, 1));
    assert_eq!(diagonal.length(), 1);
    assert_eq!(Direction::of([1, 2]).unwrap().length(), 2);
    // (2, 3, 6) is 7 long: (0, 0, 7) lies 6 along it, and its cross product with the unit is
    // (−3, 2, 0), √13 apart, so 4.
    let space = Direction::of([2, 3, 6]).unwrap();
    assert_eq!(space.length(), 7);
    assert_eq!(space.along([2, 3, 6]), 7);
    assert_eq!((space.along([0, 0, 7]), space.apart([0, 0, 7])), (6, 4));
    // A vector of a length rounds each axis away from zero past the unit's own rounding, so an
    // axis whose true value is whole ends a unit past it: 5 along (3, 4) is (4, 5). Along an axis
    // the unit is exact, so 5 along (7, 0) is (5, 0).
    assert_eq!(tilted.scaled_away(5), [4, 5]);
    assert_eq!(Direction::of([-3, -4]).unwrap().scaled_away(5), [-4, -5]);
    assert_eq!(axis.scaled_away(5), [5, 0]);
    assert_eq!(Direction::of([0, -2]).unwrap().scaled_away(5), [0, -5]);
    assert_eq!(axis.scaled_away(0), [0, 0]);
    // No length, no direction.
    assert_eq!(Direction::of([0, 0]), None);
    assert_eq!(Direction::of([0, 0, 0]), None);
    // At the widest axes no product overflows: the diagonal of the cube 2⁴⁷ − 1 on a side lies
    // along itself its length, each rounded, so within a unit of it, and not apart from itself.
    let wide = (1 << 47) - 1;
    let cube = Direction::of([wide; 3]).unwrap();
    assert!((cube.along([wide; 3]) - cube.length()).abs() <= 1);
    assert_eq!(cube.apart([wide; 3]), 0);
    assert_eq!(cube.along([-wide; 3]), -cube.along([wide; 3]));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    /// In two axes each measure lies within 0.51 of a unit of the true value; the reference adds
    /// at most 2⁻¹⁰.
    #[test]
    fn a_flat_direction_measures_within_its_bound(
        vector in prop::array::uniform2(-(1_i128 << 40)..(1 << 40)),
        off in prop::array::uniform2(-(1_i128 << 40)..(1 << 40)),
    ) {
        let Some(direction) = Direction::of(vector) else { return Ok(()) };
        let [length, along, apart] = errors(&direction, vector, off, direction.apart(off));
        prop_assert!(length <= 0.501 && along <= 0.511 && apart <= 0.511, "{length} {along} {apart}");
        let reach = off[0].abs();
        prop_assert!(scaled_within(vector, reach, direction.scaled_away(reach)));
    }

    /// In three axes the length and along lie within 0.51 of a unit, and apart within 1.4.
    #[test]
    fn a_direction_in_space_measures_within_its_bound(
        vector in prop::array::uniform3(-(1_i128 << 40)..(1 << 40)),
        off in prop::array::uniform3(-(1_i128 << 40)..(1 << 40)),
    ) {
        let Some(direction) = Direction::of(vector) else { return Ok(()) };
        let [length, along, apart] = errors(&direction, vector, off, direction.apart(off));
        prop_assert!(length <= 0.501 && along <= 0.511 && apart <= 1.4, "{length} {along} {apart}");
        let reach = off[0].abs();
        prop_assert!(scaled_within(vector, reach, direction.scaled_away(reach)));
    }

    /// Short vectors, whose length's root takes the most bits of fraction, keep the bound.
    #[test]
    fn a_short_direction_measures_within_its_bound(
        vector in prop::array::uniform3(-8_i128..8),
        off in prop::array::uniform3(-(1_i128 << 40)..(1 << 40)),
    ) {
        let Some(direction) = Direction::of(vector) else { return Ok(()) };
        let [length, along, apart] = errors(&direction, vector, off, direction.apart(off));
        prop_assert!(length <= 0.501 && along <= 0.511 && apart <= 1.4, "{length} {along} {apart}");
    }
}
