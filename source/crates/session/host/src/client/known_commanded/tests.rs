use campfire_codec::Binary;

use super::*;

fn ids(values: &[u8]) -> Vec<StableId> {
    values
        .iter()
        .map(|&value| Binary::decode(&[value]).unwrap())
        .collect()
}

#[test]
fn the_client_knows_the_units_of_the_last_whole_select_the_server_applied() {
    let (a, b) = (ids(&[1, 2]), ids(&[3]));
    let applied = |tick| Some(Tick::new(tick));
    let mut known = KnownCommanded::default();
    // At the start it knows nothing; a select of `a` in seq 4 waits for its ack.
    assert!(!known.commands(&a));
    known.sent(4, &a);
    assert!(!known.commands(&a));
    // The server applied 4: `a` alone needs no select.
    known.acknowledge(4, &[applied(10)]);
    assert!(known.commands(&a) && !known.commands(&b));
    // A select of `b` in seq 6 the server logged early changes nothing once its ack says so; while
    // it waits, no order goes without a select.
    known.sent(6, &b);
    assert!(!known.commands(&a));
    known.acknowledge(5, &[applied(11), None]);
    assert!(known.commands(&a) && !known.commands(&b));
    // A part of a select past one payload, applied, leaves the client not knowing; another part
    // never applied changes nothing, and an ack of seqs it never sent passes them over.
    known.sent_part(7);
    known.sent_part(8);
    known.acknowledge(7, &[applied(12), None, applied(12)]);
    assert!(!known.commands(&a) && !known.commands(&b));
    known.sent(9, &b);
    known.acknowledge(9, &[applied(13)]);
    assert!(known.commands(&b));
    // A failed link forgets it all.
    known.clear();
    assert!(!known.commands(&b));
}
