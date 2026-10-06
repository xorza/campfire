use super::*;
fn id(value: u8) -> StableId {
    postcard::from_bytes(&[value]).unwrap()
}

#[test]
fn orders_decode_exactly() {
    let order = Order {
        unit: id(4),
        action: Action::Move {
            x: Num::int(-3),
            z: Num::from_bits(5),
        },
    };
    let body = order.encode();
    assert_eq!(Order::decode(&body), Some(order));
    assert_eq!(Order::decode(&[body.as_slice(), &[0]].concat()), None);
    assert_eq!(Order::decode(&body[..body.len() - 1]), None);
    assert_eq!(Order::decode(&[]), None);

    // The unit's id, then variant 1 with the target's id, each a varint: 300 = 0xAC 0x02.
    let target = postcard::from_bytes::<StableId>(&[0xAC, 0x02]).unwrap();
    let attack = Order {
        unit: target,
        action: Action::Attack { target },
    };
    assert_eq!(attack.encode(), [0xAC, 0x02, 1, 0xAC, 0x02]);
    assert_eq!(Order::decode(&[0xAC, 0x02, 1, 0xAC, 0x02]), Some(attack));
    // Variant 2 does not exist.
    assert_eq!(Order::decode(&[0, 2, 0]), None);

    // A payload is a list of `orders` commands, one per order.
    let payload = Order::payload(&[order, attack]);
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
    // its 5 body bytes; then the move's payload, the body buffer reused.
    let mut out = vec![9];
    let mut body = Vec::new();
    attack.write_payload(&mut body, &mut out);
    assert_eq!(out, [9, 1, 5, 5, 0xAC, 0x02, 1, 0xAC, 0x02]);
    order.write_payload(&mut body, &mut out);
    assert_eq!(out[9..], Order::payload(&[order]));
}
