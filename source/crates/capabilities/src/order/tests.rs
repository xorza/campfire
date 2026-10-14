use campfire_items::ItemId;
use campfire_math::Num;

use super::*;

fn id(value: u8) -> StableId {
    Binary::decode(&[value]).unwrap()
}

/// The action of `body` as `A`'s orders decode it, with its units; `None` for a body that is no
/// such order.
fn decode<A: CapabilityAction>(body: &[u8]) -> Option<(Vec<StableId>, A)> {
    let mut units = Vec::new();
    let action = A::decode_into(body, &mut units)?;
    Some((units, action))
}

#[test]
fn orders_decode_exactly() {
    let walk = OrdersAction::Move {
        x: Num::int(-3),
        z: Num::from_bits(5),
    };
    let order = Order::one(id(4), walk);
    let body = order.encode();
    assert_eq!(decode(&body), Some((vec![id(4)], walk)));
    assert_eq!(
        decode::<OrdersAction>(&[body.as_slice(), &[0]].concat()),
        None
    );
    assert_eq!(decode::<OrdersAction>(&body[..body.len() - 1]), None);
    assert_eq!(decode::<OrdersAction>(&[]), None);

    // The count of units, then each unit's id, then variant 1 with the target's id, each a
    // varint: 300 = 0xAC 0x02.
    let target = Binary::decode::<StableId>(&[0xAC, 0x02]).unwrap();
    let attack = Order::one(target, OrdersAction::Attack { target });
    assert_eq!(attack.encode(), [1, 0xAC, 0x02, 1, 0xAC, 0x02]);
    assert_eq!(
        decode(&[1, 0xAC, 0x02, 1, 0xAC, 0x02]),
        Some((vec![target], OrdersAction::Attack { target }))
    );
    // `orders` has 5 variants, 0 to 4.
    assert_eq!(decode::<OrdersAction>(&[1, 0, 5]), None);

    // Each owner's action follows the units in its own format: a learn of slot 2, variant 0;
    // a cancel of the train at place 3, variant 0; a rally cleared, variant 1 and `None`; a swap
    // of slots 0 and 1, variant 2.
    let learn = Order::one(id(4), ProgressionAction::Learn { slot: 2 });
    let cancel = Order::one(id(4), ProductionAction::CancelTrain { place: 3 });
    let rally = Order::one(id(4), ProductionAction::Rally { target: None });
    let swap = Order::one(id(4), ItemsAction::Swap { from: 0, to: 1 });
    assert_eq!(learn.encode(), [1, 4, 0, 2]);
    assert_eq!(cancel.encode(), [1, 4, 0, 3]);
    assert_eq!(rally.encode(), [1, 4, 1, 0]);
    assert_eq!(swap.encode(), [1, 4, 2, 0, 1]);
    assert_eq!(
        decode(&swap.encode()),
        Some((vec![id(4)], ItemsAction::Swap { from: 0, to: 1 }))
    );

    // A payload is a list of commands, one per order, each to its action's capability.
    let orders = [order.clone(), attack.clone(), learn, cancel, swap];
    let payload = Order::payload(&orders);
    let mut commands = Vec::new();
    assert!(Command::read(&payload, |command, _| commands.push(command)));
    let capabilities: Vec<Capability> = commands.iter().map(|command| command.capability).collect();
    assert_eq!(
        capabilities,
        [
            Capability::Orders,
            Capability::Orders,
            Capability::Progression,
            Capability::Production,
            Capability::Items,
        ]
    );
    assert_eq!(commands[1].body, attack.encode());
    assert_eq!(commands[4].body, orders[4].encode());

    // A written payload follows what the buffer holds: 1 command, of capability 5 (`orders`),
    // its 6 body bytes; then the learn's, of capability 13 (`progression`), its 4 bytes; then the
    // move's payload, the body buffer reused.
    let mut out = vec![9];
    let mut body = Vec::new();
    attack.write_payload(&mut body, &mut out);
    assert_eq!(out, [9, 1, 5, 6, 1, 0xAC, 0x02, 1, 0xAC, 0x02]);
    orders[2].write_payload(&mut body, &mut out);
    assert_eq!(out[10..], [1, 13, 4, 1, 4, 0, 2]);
    order.write_payload(&mut body, &mut out);
    assert_eq!(out[17..], Order::payload(&[order]));
}

#[test]
fn an_order_names_its_units_in_increasing_stable_id_each_once() {
    // A stop, variant 4, to units 2, 5 and 9.
    let group = Order {
        units: OrderUnits::new(&[id(2), id(5), id(9)]).unwrap(),
        action: Action::Orders(OrdersAction::Stop),
    };
    assert_eq!(group.encode(), [3, 2, 5, 9, 4]);
    assert_eq!(
        decode(&[3, 2, 5, 9, 4]),
        Some((vec![id(2), id(5), id(9)], OrdersAction::Stop))
    );
    // No unit, units out of order, a unit twice, and a count past the bytes left decode to
    // nothing, and leave a buffer as it was.
    let mut units = vec![id(1)];
    for flawed in [
        &[0, 4][..],
        &[2, 5, 2, 4],
        &[2, 5, 5, 4],
        &[3, 2, 5, 4],
        &[200, 2, 5, 4],
    ] {
        assert_eq!(
            OrdersAction::decode_into(flawed, &mut units),
            None,
            "{flawed:?}"
        );
        assert_eq!(units, [id(1)]);
    }
    assert_eq!(
        OrdersAction::decode_into(&[2, 2, 5, 4], &mut units),
        Some(OrdersAction::Stop)
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
fn each_action_is_a_command_of_its_owner() {
    let item = ItemId::nth(0);
    let owned = [
        (Action::from(OrdersAction::Stop), Capability::Orders),
        (
            Action::from(ProgressionAction::Learn { slot: 0 }),
            Capability::Progression,
        ),
        (
            Action::from(ProductionAction::CancelBuild),
            Capability::Production,
        ),
        (
            Action::from(ItemsAction::Buy { item, shop: id(1) }),
            Capability::Items,
        ),
    ];
    for (action, capability) in owned {
        assert_eq!(action.capability(), capability, "{action:?}");
    }
}
