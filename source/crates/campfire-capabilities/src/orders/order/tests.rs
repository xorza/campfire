use super::*;
fn id(value: u8) -> StableId {
    Binary::decode(&[value]).unwrap()
}

#[test]
fn orders_decode_exactly() {
    let order = Order::one(
        id(4),
        Action::Move {
            x: Num::int(-3),
            z: Num::from_bits(5),
        },
    );
    let body = order.encode();
    assert_eq!(Order::decode(&body), Some(order.clone()));
    assert_eq!(Order::decode(&[body.as_slice(), &[0]].concat()), None);
    assert_eq!(Order::decode(&body[..body.len() - 1]), None);
    assert_eq!(Order::decode(&[]), None);

    // The count of units, then each unit's id, then variant 1 with the target's id, each a
    // varint: 300 = 0xAC 0x02.
    let target = Binary::decode::<StableId>(&[0xAC, 0x02]).unwrap();
    let attack = Order::one(target, Action::Attack { target });
    assert_eq!(attack.encode(), [1, 0xAC, 0x02, 1, 0xAC, 0x02]);
    assert_eq!(
        Order::decode(&[1, 0xAC, 0x02, 1, 0xAC, 0x02]),
        Some(attack.clone())
    );
    // Variant 9 does not exist.
    assert_eq!(Order::decode(&[1, 0, 9]), None);

    // A payload is a list of `orders` commands, one per order.
    let payload = Order::payload(&[order.clone(), attack.clone()]);
    let mut commands = Vec::new();
    assert!(Command::read(&payload, |command, _| commands.push(command)));
    assert_eq!(commands.len(), 2);
    assert!(
        commands
            .iter()
            .all(|command| command.capability == Capability::Orders)
    );
    assert_eq!(commands[1].body, attack.encode());

    // A written payload follows what the buffer holds: 1 command, of capability 5 (`orders`),
    // its 6 body bytes; then the move's payload, the body buffer reused.
    let mut out = vec![9];
    let mut body = Vec::new();
    attack.write_payload(&mut body, &mut out);
    assert_eq!(out, [9, 1, 5, 6, 1, 0xAC, 0x02, 1, 0xAC, 0x02]);
    order.write_payload(&mut body, &mut out);
    assert_eq!(out[10..], Order::payload(&[order]));
}

#[test]
fn an_order_names_its_units_in_increasing_stable_id_each_once() {
    // A stop, variant 7, to units 2, 5 and 9.
    let group = Order {
        units: OrderUnits::new(&[id(2), id(5), id(9)]).unwrap(),
        action: Action::Stop,
    };
    assert_eq!(group.encode(), [3, 2, 5, 9, 7]);
    assert_eq!(Order::decode(&[3, 2, 5, 9, 7]), Some(group));
    // No unit, units out of order, a unit twice, and a count past the bytes left decode to
    // nothing, and leave a buffer as it was.
    let mut units = vec![id(1)];
    for flawed in [
        &[0, 7][..],
        &[2, 5, 2, 7],
        &[2, 5, 5, 7],
        &[3, 2, 5, 7],
        &[200, 2, 5, 7],
    ] {
        assert_eq!(Order::decode(flawed), None, "{flawed:?}");
        assert_eq!(Order::decode_into(flawed, &mut units), None, "{flawed:?}");
        assert_eq!(units, [id(1)]);
    }
    assert_eq!(
        Order::decode_into(&[2, 2, 5, 7], &mut units),
        Some(Action::Stop)
    );
    assert_eq!(units, [id(1), id(2), id(5)]);
    // The list type holds the same rule.
    assert_eq!(OrderUnits::new(&[]), None);
    assert_eq!(OrderUnits::new(&[id(5), id(2)]), None);
    assert_eq!(OrderUnits::new(&[id(5), id(5)]), None);
    assert_eq!(OrderUnits::new(&[id(5)]), Some(OrderUnits::one(id(5))));
    assert_eq!(
        OrderUnits::new(&[id(2), id(5)]).unwrap().get(),
        [id(2), id(5)]
    );
}

#[test]
fn only_the_cores_actions_go_to_each_unit() {
    let item = ItemId::nth(0);
    let target = BuildTarget::Point {
        x: Num::ZERO,
        z: Num::ZERO,
        angle: Num::ZERO,
    };
    let to_units = [
        Action::Move {
            x: Num::ZERO,
            z: Num::ZERO,
        },
        Action::Attack { target: id(4) },
        Action::Slot {
            slot: 0,
            target: ActionTarget::None,
        },
        Action::Build { slot: 0, target },
        Action::Stop,
    ];
    let elsewhere = [
        Action::Learn { slot: 0 },
        Action::Buy { item },
        Action::Sell { slot: 0 },
        Action::Swap { from: 0, to: 1 },
        Action::CancelTrain { place: 0 },
        Action::Rally { target: None },
        Action::CancelBuild,
    ];
    assert!(to_units.iter().all(|action| action.to_units()));
    assert!(!elsewhere.iter().any(|action| action.to_units()));
}
