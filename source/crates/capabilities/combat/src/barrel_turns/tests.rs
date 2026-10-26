use campfire_codec::Binary;

use super::*;

#[test]
fn a_weapons_barrel_turns_with_each_shot_back_to_its_first_and_holds_only_past_it() {
    // Of three barrels, the shots take 0, 1, 2, then 0 again; a weapon at its first is not held.
    let [rockets, cannon] = [ActionId::new(3), ActionId::new(1)];
    let mut turns = BarrelTurns::default();
    let taken: Vec<usize> = (0..4).map(|_| turns.turn(rockets, 3)).collect();
    assert_eq!(taken, [0, 1, 2, 0]);
    assert_eq!(
        turns,
        BarrelTurns(vec![Turned {
            action: rockets,
            barrel: 1
        }])
    );
    // Another weapon's turn goes in the order of the ids; a weapon its type now gives one barrel
    // takes its first and is held no more.
    let actions = |turns: &BarrelTurns| -> Vec<ActionId> {
        turns.0.iter().map(|turned| turned.action).collect()
    };
    assert_eq!(turns.turn(cannon, 2), 0);
    assert_eq!(actions(&turns), [cannon, rockets]);
    assert_eq!(turns.turn(cannon, 1), 0);
    assert_eq!(actions(&turns), [rockets]);
}

#[test]
fn a_snapshots_barrel_turns_decode_in_order_each_past_its_first() {
    let turned = |action: u32, barrel: u16| Turned {
        action: ActionId::new(action),
        barrel,
    };
    let decoded = |turns: Vec<Turned>| Binary::decode::<BarrelTurns>(&Binary::encode(&turns));
    assert!(decoded(vec![turned(1, 1), turned(2, 3)]).is_ok());
    assert!(decoded(vec![turned(2, 1), turned(1, 1)]).is_err());
    assert!(decoded(vec![turned(1, 1), turned(1, 2)]).is_err());
    assert!(decoded(vec![turned(1, 0)]).is_err());
}
