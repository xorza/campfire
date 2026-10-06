use std::array;

use campfire_capabilities::{Action, Order};
use campfire_math::Num;
use campfire_sim::IdAllocator;

use super::*;

fn at(sent: &SentInputs, stamp: u64) -> Vec<&[u8]> {
    sent.at(Tick::new(stamp)).collect()
}

#[test]
fn the_inputs_keep_their_stamps_and_payloads_from_the_oldest_on() {
    let mut ids = IdAllocator::default();
    let units: [_; 4] = array::from_fn(|_| ids.allocate());
    let attack = |unit| Order {
        unit,
        action: Action::Attack { target: unit },
    };
    // Units 0 to 3, each id a 1-byte varint: 1 command, of capability 5, with its 3 body
    // bytes, the unit, variant 1 and the target.
    let payload = |unit: u8| [1, 5, 3, unit, 1, unit];
    let mut sent = SentInputs::default();
    let push = |sent: &mut SentInputs, stamp, order: &Order| {
        sent.push(Tick::new(stamp), 6, |body, out| {
            order.write_payload(body, out);
        })
    };
    for (stamp, unit) in [(1, 0), (1, 1), (2, 2), (4, 3)] {
        assert!(push(&mut sent, stamp, &attack(units[unit])));
    }
    // A coordinate of `i64::MAX` is a 10-byte varint, so the payload passes 6 bytes.
    let far = Order {
        unit: units[0],
        action: Action::Move {
            x: Num::from_bits(i64::MAX),
            z: Num::from_bits(0),
        },
    };
    assert!(!push(&mut sent, 4, &far));
    // Dropped wholly: the four payloads before it alone stay.
    assert_eq!(sent.payloads.len(), 4 * 6);
    let none: [[u8; 6]; 0] = [];
    assert_eq!(at(&sent, 1), [payload(0), payload(1)]);
    assert_eq!(at(&sent, 3), none);
    assert_eq!(at(&sent, 4), [payload(3)]);
    assert_eq!(sent.since(2).collect::<Vec<_>>(), [payload(2), payload(3)]);

    sent.prune(Tick::new(0));
    sent.prune(Tick::new(1));
    assert_eq!(sent.len(), 4);
    sent.prune(Tick::new(2));
    assert_eq!((sent.len(), sent.payloads.len()), (2, 12));
    assert_eq!(at(&sent, 1), none);
    assert_eq!(at(&sent, 2), [payload(2)]);
    assert_eq!(at(&sent, 4), [payload(3)]);
    sent.prune(Tick::new(5));
    assert_eq!((sent.len(), sent.payloads.len()), (0, 0));
    assert!(push(&mut sent, 6, &attack(units[1])));
    sent.clear();
    assert_eq!((sent.len(), sent.payloads.len()), (0, 0));
}
