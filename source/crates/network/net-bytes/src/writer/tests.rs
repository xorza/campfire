use super::*;

fn message(writer: &mut Writer, len: usize, fill: u8) -> Bytes {
    writer.write_bytes(&vec![fill; len]);
    writer.take_written()
}

/// Messages of 1024 bytes fill a 4096-byte chunk four at a time, so the ninth message needs a
/// third chunk. Once the first chunk's four messages are dropped, the ninth starts at the first
/// chunk's address; while one of them is alive, it gets a new allocation.
#[test]
fn a_chunk_is_reused_once_its_messages_are_dropped() {
    let mut writer = Writer::default();
    let first: Vec<Bytes> = (0..4).map(|i| message(&mut writer, 1024, i)).collect();
    let base = first[0].as_ptr();
    assert_eq!(first[3].as_ptr(), base.wrapping_add(3 * 1024));
    let second: Vec<Bytes> = (4..8).map(|i| message(&mut writer, 1024, i)).collect();
    assert_ne!(second[0].as_ptr(), base.wrapping_add(4 * 1024));
    drop(first);

    let ninth = message(&mut writer, 1024, 8);
    assert_eq!(ninth.as_ptr(), base);
    assert_eq!(&*ninth, &[8; 1024][..]);

    let mut kept = Writer::default();
    let alive = message(&mut kept, 1024, 0);
    let kept_base = alive.as_ptr();
    let rest: Vec<Bytes> = (1..8).map(|i| message(&mut kept, 1024, i)).collect();
    let ninth = message(&mut kept, 1024, 8);
    assert_ne!(ninth.as_ptr(), kept_base);
    drop(rest);
}

/// The first chunk switch makes the current chunk and a spare: messages of 1024 bytes fill the
/// first chunk four at a time, and the fifth, while the first four are alive, starts at the
/// spare's address instead of a new allocation.
#[test]
fn the_first_switch_makes_the_spare_the_second_takes() {
    let mut writer = Writer::default();
    let first = message(&mut writer, 1024, 0);
    assert_eq!(writer.arena.spares(), 1);
    assert_eq!(writer.arena.allocations(), 2);
    let rest: Vec<Bytes> = (1..4).map(|i| message(&mut writer, 1024, i)).collect();

    let fifth = message(&mut writer, 1024, 4);
    assert_eq!(writer.arena.allocations(), 2);
    assert_eq!(&*fifth, &[4; 1024][..]);
    drop((first, rest));
}

/// A message whose second write does not fit the 96 bytes left of a chunk moves to the next
/// chunk whole: its 25 + 100 bytes stay in order.
#[test]
fn a_message_that_crosses_a_chunk_moves_whole() {
    let mut writer = Writer::default();
    let head = message(&mut writer, 4000, 1);
    writer.write_bytes(&[2; 25]);
    writer.write_bytes(&[3; 100]);
    let crossing = writer.take_written();
    assert_eq!(crossing.len(), 125);
    assert_eq!(crossing[..25], [2; 25]);
    assert_eq!(crossing[25..], [3; 100]);
    assert_ne!(crossing.as_ptr(), head.as_ptr().wrapping_add(4000));
}

/// A message above the 4096-byte chunk gets its own allocation, and the writer goes on in chunks
/// after it.
#[test]
fn a_message_above_a_chunk_gets_its_own_allocation() {
    let mut writer = Writer::default();
    let large = message(&mut writer, CHUNK + 1, 4);
    assert_eq!(large.len(), CHUNK + 1);
    assert!(large.iter().all(|&byte| byte == 4));
    let small = message(&mut writer, 3, 5);
    assert_eq!(&*small, &[5, 5, 5][..]);
}

#[test]
fn reset_drops_the_message_being_written() {
    let mut writer = Writer::default();
    writer.write_bytes(&[1, 2, 3]);
    writer.reset();
    assert!(writer.is_empty());
    writer.write_bytes(&[4]);
    assert_eq!(&*writer.take_written(), &[4][..]);
}

/// Once the messages of the only chunk are dropped, a message that does not fit its end starts
/// at the chunk's own address: 4000 bytes, then 200 more, reuse the one 4096-byte chunk.
#[test]
fn a_chunk_whose_messages_are_dropped_is_reused_in_place() {
    let mut writer = Writer::default();
    let first = message(&mut writer, 4000, 1);
    let base = first.as_ptr();
    drop(first);
    let second = message(&mut writer, 200, 2);
    assert_eq!(second.as_ptr(), base);
    assert_eq!(&*second, &[2; 200][..]);
}

/// A message that crosses into the second chunk leaves the first chunk reusable: with the
/// crossing message and the 4000-byte one dropped, the third chunk is the first one again.
#[test]
fn a_chunk_left_by_a_crossing_message_is_reused() {
    let mut writer = Writer::default();
    let head = message(&mut writer, 4000, 1);
    let base = head.as_ptr();
    writer.write_bytes(&[2; 50]);
    writer.write_bytes(&[3; 100]);
    let crossing = writer.take_written();
    let held: Vec<Bytes> = (0..3).map(|i| message(&mut writer, 1300, i)).collect();
    drop((head, crossing));
    let next = message(&mut writer, 1300, 9);
    assert_eq!(next.as_ptr(), base);
    drop(held);
}

/// `fill` appends the bytes it writes after what the message holds, and leaves the message as it
/// was when it fails.
#[test]
fn fill_appends_what_it_writes_and_nothing_on_failure() {
    let mut writer = Writer::default();
    writer.write_bytes(&[1]);
    writer
        .fill(3, |out| {
            assert_eq!(out, [0, 0, 0]);
            out.copy_from_slice(&[2, 3, 4]);
            Ok::<(), ()>(())
        })
        .unwrap();
    assert_eq!(writer.fill(2, |_| Err(())), Err(()));
    assert_eq!(&*writer.take_written(), &[1, 2, 3, 4][..]);
}
