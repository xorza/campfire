use std::{array, slice};

use campfire_capabilities::Order;
use campfire_math::Num;
use campfire_orders::OrdersDirective;
use campfire_sim::IdAllocator;
use campfire_units::GroundPoint;

use super::*;

fn at(sent: &SentInputs, stamp: u64) -> Vec<&[u8]> {
    sent.at(Tick::new(stamp)).collect()
}

#[test]
fn the_inputs_keep_their_stamps_and_payloads_from_the_oldest_on() {
    let mut ids = IdAllocator::default();
    let units: [_; 4] = array::from_fn(|_| ids.allocate());
    let attack = |unit| Order::one(unit, OrdersDirective::Attack { target: unit });
    // Units 0 to 3, each id a 1-byte varint: 2 commands, the select to the players, 0, with its
    // 3 body bytes, a new select, its count of units and the unit; then the attack to capability
    // 5, 1 and 5, with its 2 body bytes, variant 1 and the target.
    let payload = |unit: u8| [2, 0, 3, 0, 1, unit, 1, 5, 2, 1, unit];
    let mut sent = SentInputs::default();
    let push = |sent: &mut SentInputs, seq, stamp, order: &Order| {
        sent.push(seq, Tick::new(stamp), 11, |_, out| {
            out.extend_from_slice(&Order::payload(slice::from_ref(order)));
        });
    };
    for (seq, (stamp, unit)) in [(1, 0), (1, 1), (2, 2), (4, 3)].into_iter().enumerate() {
        push(&mut sent, seq as u64, stamp, &attack(units[unit]));
    }
    assert_eq!(sent.payloads.len(), 4 * 11);
    let none: [[u8; 11]; 0] = [];
    assert_eq!(at(&sent, 1), [payload(0), payload(1)]);
    assert_eq!(at(&sent, 3), none);
    assert_eq!(at(&sent, 4), [payload(3)]);
    assert_eq!(sent.since(2).collect::<Vec<_>>(), [payload(2), payload(3)]);

    sent.prune(Tick::new(0));
    sent.prune(Tick::new(1));
    assert_eq!(sent.len(), 4);
    sent.prune(Tick::new(2));
    assert_eq!((sent.len(), sent.payloads.len()), (2, 22));
    assert_eq!(at(&sent, 1), none);
    assert_eq!(at(&sent, 2), [payload(2)]);
    assert_eq!(at(&sent, 4), [payload(3)]);
    sent.prune(Tick::new(5));
    assert_eq!((sent.len(), sent.payloads.len()), (0, 0));
    push(&mut sent, 4, 6, &attack(units[1]));
    sent.clear();
    assert_eq!((sent.len(), sent.payloads.len()), (0, 0));
}

#[test]
fn an_acknowledged_input_replays_where_the_server_applied_it() {
    let mut ids = IdAllocator::default();
    let units: [_; 6] = array::from_fn(|_| ids.allocate());
    let attack = |unit| Order::one(unit, OrdersDirective::Attack { target: unit });
    let payload = |unit: u8| [2, 0, 3, 0, 1, unit, 1, 5, 2, 1, unit];
    let mut sent = SentInputs::default();
    // Seqs 10 to 15, stamped 1, 1, 2, 4, 4 and 5.
    for (seq, stamp) in (10..).zip([1, 1, 2, 4, 4, 5]) {
        let order = attack(units[usize::try_from(seq - 10).unwrap()]);
        sent.push(seq, Tick::new(stamp), 11, |_, out| {
            out.extend_from_slice(&Order::payload(slice::from_ref(&order)));
        });
    }
    let none: [[u8; 11]; 0] = [];
    // The server applied 10 at its stamp, logged 11 early, and applied 12 in tick 3, a tick past
    // its stamp: 11 never replays, 12 replays in 3, and 13 to 15, not acknowledged yet, stay at
    // their stamps, behind it.
    sent.acknowledge(10, &[Some(Tick::new(1)), None, Some(Tick::new(3))]);
    assert_eq!(at(&sent, 1), [payload(0)]);
    assert_eq!(at(&sent, 2), none);
    assert_eq!(at(&sent, 3), [payload(2)]);
    assert_eq!(at(&sent, 4), [payload(3), payload(4)]);
    // 13 applied in tick 6: 14 and 15, which the server applies after it in chain order, are
    // expected there too, past 15's stamp of 5.
    sent.acknowledge(13, &[Some(Tick::new(6))]);
    assert_eq!(at(&sent, 4), none);
    assert_eq!(at(&sent, 5), none);
    assert_eq!(at(&sent, 6), [payload(3), payload(4), payload(5)]);
    // 14 never applies, and 15 in 7. A seq the client never sent, or one it no longer keeps,
    // changes nothing.
    sent.acknowledge(14, &[None, Some(Tick::new(7)), Some(Tick::new(9))]);
    sent.acknowledge(2, &[Some(Tick::new(9))]);
    assert_eq!(at(&sent, 6), [payload(3)]);
    assert_eq!(at(&sent, 7), [payload(5)]);
    assert_eq!(at(&sent, 9), none);
    // Pruning goes by the tick each takes effect in: what takes effect before 6 goes.
    sent.prune(Tick::new(6));
    assert_eq!(sent.len(), 3);
    assert_eq!(at(&sent, 6), [payload(3)]);
}

#[test]
#[should_panic(expected = "an input of 21 bytes passes the session's 11")]
fn an_input_past_the_max_length_is_a_logic_error() {
    // A coordinate of `i64::MAX` is a 10-byte varint: a move's select of 3 bytes and its
    // directive's 11, with their commands' heads, take 21 bytes, past 11.
    let unit = IdAllocator::default().allocate();
    let far = Order::one(
        unit,
        OrdersDirective::Move {
            to: GroundPoint {
                x: Num::from_bits(i64::MAX),
                z: Num::from_bits(0),
            },
        },
    );
    SentInputs::default().push(0, Tick::new(0), 11, |_, out| {
        out.extend_from_slice(&Order::payload(slice::from_ref(&far)));
    });
}
