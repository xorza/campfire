use std::slice;

use campfire_actions::ActionTarget;
use campfire_math::{Num, Vec3};
use campfire_production::RallyTarget;
use campfire_units::{BuildTarget, GroundPoint};
use campfire_values::ItemId;

use crate::Release;

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
    // (04-capabilities/control.md, A player's orders go to its commanded units): variant 1 with the
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
    let orders = [order, attack.clone(), learn, cancel, swap];
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

    // An order that fits one payload writes one: 2 commands, the select to the players, 0, its 4
    // bytes, then the attack to capability 5 (`orders`), 1 and 5, its 3 bytes, as `payload`
    // writes it; then the learn's, the buffers reused, its select of unit 4 in 3 bytes and its 2
    // bytes to capability 9 (`progression`). With no select, the attack is its command alone.
    let mut payloads = OrderPayloads::default();
    let written = |payloads: &OrderPayloads| -> Vec<Vec<u8>> {
        payloads.iter().map(<[u8]>::to_vec).collect()
    };
    assert!(attack.write_payloads(1024, &mut payloads));
    assert_eq!(
        written(&payloads),
        [vec![2, 0, 4, 0, 1, 0xAC, 0x02, 1, 5, 3, 1, 0xAC, 0x02]]
    );
    assert_eq!(
        written(&payloads),
        [Order::payload(slice::from_ref(&attack))]
    );
    assert!(orders[2].write_payloads(1024, &mut payloads));
    assert_eq!(written(&payloads), [vec![2, 0, 3, 0, 1, 4, 1, 9, 2, 0, 2]]);
    let mut out = vec![9];
    let mut body = Vec::new();
    attack.write_commanded_payload(&mut body, &mut out);
    assert_eq!(out, [9, 1, 1, 5, 3, 1, 0xAC, 0x02]);
}

#[test]
fn an_order_past_one_payload_splits_its_select_and_the_last_part_holds_its_directive() {
    // A stop, variant 4, to units 0 to 9, each id a byte: a payload of k units and no directive
    // is the count of commands, the players, the body's length, the selecting and the count of
    // units, 5 bytes, and the k ids; the stop adds its capability, 1 and 5, its length and its
    // byte, 4 more (04-capabilities/control.md, A select holds what its player controls).
    let units: Vec<StableId> = (0..10).map(id).collect();
    let stop = Order {
        units: OrderUnits::new(&units).unwrap(),
        directive: Directive::Orders(OrdersDirective::Stop),
    };
    let mut payloads = OrderPayloads::default();
    let lengths =
        |payloads: &OrderPayloads| -> Vec<usize> { payloads.iter().map(<[u8]>::len).collect() };
    // Whole, 5 + 10 + 4 = 19 bytes.
    assert!(stop.write_payloads(19, &mut payloads));
    assert_eq!(lengths(&payloads), [19]);
    // At 12, the most a first part holds with one unit left is 7, 12 bytes; the last 3 and the
    // stop are 12 too, a select that continues it, 1.
    assert!(stop.write_payloads(12, &mut payloads));
    let parts: Vec<&[u8]> = payloads.iter().collect();
    assert_eq!(parts[0], [1, 0, 9, 0, 7, 0, 1, 2, 3, 4, 5, 6]);
    assert_eq!(parts[1], [2, 0, 5, 1, 3, 7, 8, 9, 1, 5, 1, 4]);
    assert_eq!(payloads.iter().len(), 2);
    // At 10: 5 units, then 4, then the last with the stop, 1 + 9 = 10 bytes.
    assert!(stop.write_payloads(10, &mut payloads));
    assert_eq!(lengths(&payloads), [10, 9, 10]);
    // At 9, one unit and the stop pass it: no payload.
    assert!(!stop.write_payloads(9, &mut payloads));
    assert_eq!(payloads.iter().len(), 0);
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

/// The place of `directive`'s variant among every directive's, so a variant no test lists fails to
/// compile here.
const fn variant(directive: &Directive) -> usize {
    match directive {
        Directive::Orders(directive) => match directive {
            OrdersDirective::Move { .. } => 0,
            OrdersDirective::Attack { .. } => 1,
            OrdersDirective::Slot { .. } => 2,
            OrdersDirective::Build { .. } => 3,
            OrdersDirective::Stop => 4,
            OrdersDirective::Use { .. } => 5,
            OrdersDirective::AttackMove { .. } => 6,
        },
        Directive::Progression(ProgressionDirective::Learn { .. }) => 7,
        Directive::Production(directive) => match directive {
            ProductionDirective::CancelTrain { .. } => 8,
            ProductionDirective::Rally { .. } => 9,
            ProductionDirective::CancelBuild => 10,
        },
        Directive::Items(directive) => match directive {
            ItemsDirective::Buy { .. } => 11,
            ItemsDirective::Sell { .. } => 12,
            ItemsDirective::Swap { .. } => 13,
        },
    }
}

#[test]
fn the_largest_order_to_one_unit_is_51_bytes_within_the_least_payload_length() {
    // Each directive at its largest values: an id of `u64::MAX` and a `Num` of `i64::MIN` take
    // 10 bytes, an item of `u32::MAX` 5, a slot 1. The largest are a slot's cast at a point and a
    // build at a point with its angle, 33 bytes: their variant, the slot, the target's variant
    // and three `Num`s. With a select of the one unit, 14 bytes with its command's addressee and
    // length, the directive's command of 36, and the count of commands, 51 bytes
    // (05-protocol-spec.md, A session's terms hold every input).
    let unit: StableId = Binary::decode(&Binary::encode(&u64::MAX)).unwrap();
    let item: ItemId = Binary::decode(&Binary::encode(&u32::MAX)).unwrap();
    let num = Num::from_bits(i64::MIN);
    let ground = GroundPoint { x: num, z: num };
    let point = Vec3 {
        x: num,
        y: num,
        z: num,
    };
    let directives: Vec<Directive> = vec![
        OrdersDirective::Move { to: ground }.into(),
        OrdersDirective::Attack { target: unit }.into(),
        OrdersDirective::Slot {
            slot: u8::MAX,
            target: ActionTarget::Point(point),
        }
        .into(),
        OrdersDirective::Build {
            slot: u8::MAX,
            target: BuildTarget::Point {
                at: ground,
                angle: num,
            },
        }
        .into(),
        OrdersDirective::Stop.into(),
        OrdersDirective::Use { target: unit }.into(),
        OrdersDirective::AttackMove { to: ground }.into(),
        ProgressionDirective::Learn { slot: u8::MAX }.into(),
        ProductionDirective::CancelTrain { place: u8::MAX }.into(),
        ProductionDirective::Rally {
            target: Some(RallyTarget::Point(ground)),
        }
        .into(),
        ProductionDirective::CancelBuild.into(),
        ItemsDirective::Buy { item, shop: unit }.into(),
        ItemsDirective::Sell {
            stack: u8::MAX,
            shop: unit,
        }
        .into(),
        ItemsDirective::Swap {
            from: u8::MAX,
            to: u8::MAX,
        }
        .into(),
    ];
    let listed: Vec<usize> = directives.iter().map(variant).collect();
    assert_eq!(listed, (0..14).collect::<Vec<_>>());
    let mut payloads = OrderPayloads::default();
    let lengths: Vec<usize> = directives
        .into_iter()
        .map(|directive| {
            let order = Order::one(unit, directive);
            assert!(order.write_payloads(u32::MAX, &mut payloads));
            let [payload] = payloads.iter().collect::<Vec<_>>()[..] else {
                panic!("one payload of {directive:?}");
            };
            payload.len()
        })
        .collect();
    let largest = lengths.iter().copied().max().unwrap();
    assert_eq!(largest, 51);
    assert_eq!(lengths[2..4], [51, 51]);
    assert!(largest <= usize::try_from(Release::LEAST_PAYLOAD_LEN).unwrap());
}
