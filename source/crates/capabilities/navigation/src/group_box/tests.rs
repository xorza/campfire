use super::*;

#[test]
fn a_group_keeps_its_shape_only_on_a_move_away_from_its_box() {
    // Three units at (1, 2), (4, 6) and (2, 3): the box runs from (1, 2) to (4, 6), its center
    // (2.5, 4). A goal of (20, 10) lies outside, so each goes to its offset from the center
    // added to the goal: (−1.5, −2) → (18.5, 8), (1.5, 2) → (21.5, 12), (−0.5, −1) →
    // (19.5, 9). Bounds of ±21 m take the second to x = 21.
    let int = Num::int;
    let half = |value: i64| Num::int(value) / 2;
    let units = [
        Vec3::from((int(1), 0, int(2))),
        Vec3::from((int(4), 0, int(6))),
        Vec3::from((int(2), 0, int(3))),
    ];
    let group = GroupBox::of(units).unwrap();
    assert_eq!(group.center(), [half(5), int(4)]);
    let bounds = Bounds::new([int(-21), int(-21)], [int(21), int(21)]).unwrap();
    let goals = units.map(|unit| group.goal_of(unit, [int(20), int(10)], bounds));
    assert_eq!(
        goals,
        [[half(37), int(8)], [int(21), int(12)], [half(39), int(9)]]
    );
    // A goal inside the box or on its edge is every unit's goal.
    for inside in [[int(3), int(5)], [int(1), int(2)], [int(4), int(4)]] {
        assert!(
            units
                .iter()
                .all(|&unit| group.goal_of(unit, inside, bounds) == inside)
        );
    }
    // Just past the edge, the shape holds again.
    let past = [int(4) + Num::from_bits(1), int(4)];
    assert_ne!(group.goal_of(units[0], past, bounds), past);
    assert_eq!(GroupBox::of([]), None);
}

#[test]
fn the_center_rounds_toward_negative_infinity() {
    // Ends 1 and 2 bits apart: (−1 + 0) / 2 = −½ bit rounds to −1 bit, (0 + 1) / 2 = ½ to 0.
    let bit = Num::from_bits;
    let group = GroupBox::of([
        Vec3::from((bit(-1), 0, bit(0))),
        Vec3::from((bit(0), 0, bit(1))),
    ])
    .unwrap();
    assert_eq!(group.center(), [bit(-1), bit(0)]);
}
