use super::*;

/// The start of the allocation `view` lies in: its first byte's address less its offset in its
/// chunk.
fn base(view: &BytesMut, offset: usize) -> usize {
    view.as_ptr().addr() - offset
}

/// Copies of 100 bytes fill a chunk of 256 two at a time, so the third starts the next chunk; a
/// chunk whose copies all dropped is reused at its first byte, and one whose copy lives is
/// replaced by a new allocation.
#[test]
fn copies_share_a_chunk_and_a_freed_one_is_reused() {
    let mut arena = Arena::preallocated(256, 1);
    assert_eq!(arena.allocations(), 2);
    let chunk = [7_u8; 100];
    let first = arena.copy(&chunk);
    let second = arena.copy(&chunk);
    assert_eq!(base(&second, 100), base(&first, 0));
    assert_eq!(*second, chunk);
    let third = arena.copy(&chunk);
    let chunk_0 = base(&first, 0);
    assert_ne!(base(&third, 0), chunk_0);
    assert_eq!(arena.allocations(), 2);

    // The spare is chunk 0, its copies dropped: the next switch takes it whole.
    drop((first, second));
    let fourth = arena.copy(&chunk);
    let fifth = arena.copy(&chunk);
    assert_eq!(base(&fifth, 0), chunk_0);
    assert_eq!(arena.allocations(), 2);

    // Chunk 1 is the spare now and its copies live, so the next switch allocates.
    let sixth = arena.copy(&chunk);
    let seventh = arena.copy(&chunk);
    assert_ne!(base(&seventh, 0), chunk_0);
    assert_ne!(base(&seventh, 0), base(&third, 0));
    assert_eq!(arena.allocations(), 3);
    drop((third, fourth, sixth));
}

/// An arena keeps at most its spares: with none, each switch past a live chunk allocates, and
/// the chunk it left goes once its views drop.
#[test]
fn an_arena_keeps_at_most_its_spares() {
    let mut arena = Arena::new(256, 0);
    let held: Vec<BytesMut> = (0..6).map(|_| arena.copy(&[1; 100])).collect();
    assert_eq!(arena.spares(), 0);
    assert_eq!(arena.allocations(), 3);
    drop(held);
}

/// A taken view has room for its bytes and fills in place; one larger than a chunk gets an
/// allocation of its own, and the arena goes on in chunks after it.
#[test]
fn a_taken_view_fills_in_place() {
    let mut arena = Arena::new(256, 1);
    let mut view = arena.take(100);
    assert_eq!((view.len(), view.capacity()), (0, 100));
    let start = view.as_ptr();
    view.extend_from_slice(&[3; 100]);
    assert_eq!(view.as_ptr(), start);
    let next = arena.take(50);
    assert_eq!(base(&next, 100), base(&view, 0));

    let large = arena.take(300);
    assert!(large.capacity() >= 300);
    let after = arena.copy(&[4; 10]);
    assert_eq!(*after, [4; 10]);
}

/// A view being written that outgrows its chunk moves to the next whole, in order.
#[test]
fn a_view_that_crosses_a_chunk_moves_whole() {
    let mut arena = Arena::new(256, 1);
    let head = arena.copy(&[1; 200]);
    arena.extend_from_slice(&[2; 25]);
    arena.extend_from_slice(&[3; 100]);
    let crossing = arena.split();
    assert_eq!(crossing[..25], [2; 25]);
    assert_eq!(crossing[25..], [3; 100]);
    assert_ne!(base(&crossing, 0), base(&head, 0));
}

/// A new chunk size drops the kept chunks, and the next view comes from a chunk of it.
#[test]
fn a_new_chunk_size_drops_the_kept_chunks() {
    let mut arena = Arena::preallocated(128, 2);
    arena.extend_from_slice(&[5; 3]);
    arena.set_chunk(512);
    assert_eq!(arena.spares(), 0);
    assert_eq!(*arena.split(), [5; 3]);
    let view = arena.take(400);
    assert_eq!(view.capacity(), 400);
    assert_eq!(arena.chunk(), 512);
}
