use campfire_capabilities::Directive;
use campfire_codec::Binary;
use campfire_orders::OrdersDirective;
use campfire_players::OrderUnits;

use super::*;

/// A stop to `values`' units, each id a byte.
fn stop(values: &[u8]) -> Order {
    let units: Vec<StableId> = values
        .iter()
        .map(|&value| Binary::decode(&[value]).unwrap())
        .collect();
    Order {
        units: OrderUnits::new(&units).unwrap(),
        directive: Directive::Orders(OrdersDirective::Stop),
    }
}

/// The payloads `order` writes at `max_len` bytes a payload, each as its own bytes.
fn payloads(order: &Order, max_len: u32) -> Vec<Vec<u8>> {
    let mut payloads = OrderPayloads::default();
    assert!(order.write_payloads(max_len, &mut payloads));
    payloads.iter().map(<[u8]>::to_vec).collect()
}

#[test]
fn pending_orders_stamp_up_to_a_ticks_inputs_and_a_split_selects_parts_wait_in_their_order() {
    // At 10 bytes a payload a stop to units 0 to 9 splits in three, and a stop to unit 3 fits one,
    // as `Order::write_payloads` writes them (04-capabilities/control.md, A select holds what its
    // player controls).
    let group = stop(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let one = stop(&[3]);
    let parts = payloads(&group, 10);
    assert_eq!(parts.len(), 3);
    let mut pending = PendingOrders::default();
    pending.push(group);
    pending.push(one.clone());
    let mut sent = SentInputs::default();
    let mut known = KnownCommanded::default();
    let mut buffer = OrderPayloads::default();
    let mut stamp = |pending: &mut PendingOrders, known: &mut KnownCommanded, tick, room| {
        let next_seq = u64::try_from(sent.len()).unwrap();
        pending.stamp(Stamping {
            stamp: Tick::new(tick),
            next_seq,
            room,
            max_len: 10,
            sent: &mut sent,
            known,
            payloads: &mut buffer,
        });
        sent.at(Tick::new(tick))
            .map(<[u8]>::to_vec)
            .collect::<Vec<_>>()
    };
    // Tick 1 takes one input: the group's first part; its other two wait, before the stop to
    // unit 3.
    assert_eq!(stamp(&mut pending, &mut known, 1, 1), [parts[0].clone()]);
    // Tick 2 takes four: the two parts, then the stop to unit 3 with its select, as the parts
    // wait for the server's word.
    let with_select = payloads(&one, 10);
    let tick_2 = stamp(&mut pending, &mut known, 2, 4);
    assert_eq!(
        tick_2,
        [parts[1].clone(), parts[2].clone(), with_select[0].clone()]
    );
    assert!(pending.0.is_empty());
    // The server applied all four: it commands unit 3, so another stop to it goes with no select.
    let applied = |tick| Some(Tick::new(tick));
    known.acknowledge(0, &[applied(1), applied(2), applied(2), applied(2)]);
    pending.push(one.clone());
    let mut alone = Vec::new();
    one.write_commanded_payload(&mut Vec::new(), &mut alone);
    assert_eq!(stamp(&mut pending, &mut known, 3, 4), [alone]);
}
