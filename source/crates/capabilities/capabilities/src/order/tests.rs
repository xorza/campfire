use campfire_math::Num;
use campfire_units::GroundPoint;
use campfire_values::ItemId;

use super::*;

fn id(value: u8) -> StableId {
    Binary::decode(&[value]).unwrap()
}

#[test]
fn orders_decode_exactly() {
    let walk = OrdersDirective::Move {
        to: GroundPoint {
            x: Num::int(-3),
            z: Num::from_bits(5),
        },
    };
    let order = Order::one(id(4), walk);
    let body = order.encode();
    assert_eq!(OrdersDirective::decode(&body), Some(walk));
    assert_eq!(
        OrdersDirective::decode(&[body.as_slice(), &[0]].concat()),
        None
    );
    assert_eq!(OrdersDirective::decode(&body[..body.len() - 1]), None);
    assert_eq!(OrdersDirective::decode(&[]), None);

    // An order's body is its directive alone, as its units are its player's commanded units
    // (commanded-units.md, A player's orders go to its commanded units): variant 1 with the
    // target's id, a varint, 300 = 0xAC 0x02.
    let target = Binary::decode::<StableId>(&[0xAC, 0x02]).unwrap();
    let attack = Order::one(target, OrdersDirective::Attack { target });
    assert_eq!(attack.encode(), [1, 0xAC, 0x02]);
    assert_eq!(
        OrdersDirective::decode(&[1, 0xAC, 0x02]),
        Some(OrdersDirective::Attack { target })
    );
    // A use of 300, variant 5, the last, so the variants before it keep their bytes; `orders`
    // has 6 variants, 0 to 5.
    let used = Order::one(id(4), OrdersDirective::Use { target });
    assert_eq!(used.encode(), [5, 0xAC, 0x02]);
    assert_eq!(OrdersDirective::decode(&[6]), None);

    // Each owner's directive in its own format: a learn of slot 2, variant 0; a cancel of the
    // train at place 3, variant 0; a rally cleared, variant 1 and `None`; a swap of slots 0 and
    // 1, variant 2.
    let learn = Order::one(id(4), ProgressionDirective::Learn { slot: 2 });
    let cancel = Order::one(id(4), ProductionDirective::CancelTrain { place: 3 });
    let rally = Order::one(id(4), ProductionDirective::Rally { target: None });
    let swap = Order::one(id(4), ItemsDirective::Swap { from: 0, to: 1 });
    assert_eq!(learn.encode(), [0, 2]);
    assert_eq!(cancel.encode(), [0, 3]);
    assert_eq!(rally.encode(), [1, 0]);
    assert_eq!(swap.encode(), [2, 0, 1]);
    assert_eq!(
        ItemsDirective::decode(&swap.encode()),
        Some(ItemsDirective::Swap { from: 0, to: 1 })
    );

    // A payload is a list of commands, two per order: a select of its units to the players,
    // then its directive to its capability.
    let orders = [order.clone(), attack.clone(), learn, cancel, swap];
    let payload = Order::payload(&orders);
    let mut commands = Vec::new();
    assert!(Command::read(&payload, |command, _| commands.push(command)));
    let to: Vec<Addressee> = commands.iter().map(|command| command.to).collect();
    let capability = Addressee::Capability;
    assert_eq!(
        to,
        [
            Addressee::Players,
            capability(Capability::Orders),
            Addressee::Players,
            capability(Capability::Orders),
            Addressee::Players,
            capability(Capability::Progression),
            Addressee::Players,
            capability(Capability::Production),
            Addressee::Players,
            capability(Capability::Items),
        ]
    );
    // The attack's select: a new one, 0, of 1 unit, 300.
    assert_eq!(commands[2].body, [0, 1, 0xAC, 0x02]);
    assert_eq!(commands[3].body, attack.encode());
    assert_eq!(commands[9].body, orders[4].encode());
    // A test reads the payload back as its orders, and a payload with a flaw as none: a byte
    // past its commands, an order before any select, or a body no capability reads.
    assert_eq!(Order::of_payload(&payload), Some(orders.to_vec()));
    assert_eq!(
        Order::of_payload(&[payload.as_slice(), &[0]].concat()),
        None
    );
    let unselected = Command::encode(&[Command {
        to: capability(Capability::Orders),
        body: &[4],
    }]);
    assert_eq!(Order::of_payload(&unselected), None);
    let flawed = Command::encode(&[
        Command {
            to: Addressee::Players,
            body: &[0, 1, 4],
        },
        Command {
            to: capability(Capability::Orders),
            body: &[6],
        },
    ]);
    assert_eq!(Order::of_payload(&flawed), None);

    // A written payload follows what the buffer holds: 2 commands, the select to the players, 0,
    // its 4 bytes, then the attack to capability 5 (`orders`), 1 and 5, its 3 bytes; then the
    // learn's, its select of unit 4 in 3 bytes and its 2 bytes to capability 9 (`progression`);
    // then the move's payload, the body buffer reused.
    let mut out = vec![9];
    let mut body = Vec::new();
    attack.write_payload(&mut body, &mut out);
    assert_eq!(out, [9, 2, 0, 4, 0, 1, 0xAC, 0x02, 1, 5, 3, 1, 0xAC, 0x02]);
    orders[2].write_payload(&mut body, &mut out);
    assert_eq!(out[14..], [2, 0, 3, 0, 1, 4, 1, 9, 2, 0, 2]);
    order.write_payload(&mut body, &mut out);
    assert_eq!(out[25..], Order::payload(&[order]));
}

#[test]
fn a_select_names_its_units_in_increasing_stable_id_each_once() {
    // A select of units 2, 5 and 9: a new one, 0, of 3 units.
    let units = [id(2), id(5), id(9)];
    let select = Select {
        selecting: Selecting::New,
        units: &units,
    };
    let mut body = Vec::new();
    select.encode_into(&mut body);
    assert_eq!(body, [0, 3, 2, 5, 9]);
    // No unit, units out of order, a unit twice, a count past the bytes left, a selecting of
    // neither kind and a byte past the units decode to nothing, and leave a buffer as it was.
    let mut held = vec![id(1)];
    for flawed in [
        &[0, 0][..],
        &[0, 2, 5, 2],
        &[0, 2, 5, 5],
        &[0, 3, 2, 5],
        &[0, 200, 2, 5],
        &[2, 1, 4],
        &[0, 1, 4, 7],
    ] {
        assert_eq!(Select::decode_into(flawed, &mut held), None, "{flawed:?}");
        assert_eq!(held, [id(1)]);
    }
    // A select that continues one, 1, appends its units.
    assert_eq!(
        Select::decode_into(&[1, 2, 2, 5], &mut held),
        Some(Selecting::More)
    );
    assert_eq!(held, [id(1), id(2), id(5)]);
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
        (Directive::from(OrdersDirective::Stop), Capability::Orders),
        (
            Directive::from(ProgressionDirective::Learn { slot: 0 }),
            Capability::Progression,
        ),
        (
            Directive::from(ProductionDirective::CancelBuild),
            Capability::Production,
        ),
        (
            Directive::from(ItemsDirective::Buy { item, shop: id(1) }),
            Capability::Items,
        ),
    ];
    for (directive, capability) in owned {
        assert_eq!(directive.capability(), capability, "{directive:?}");
    }
}
