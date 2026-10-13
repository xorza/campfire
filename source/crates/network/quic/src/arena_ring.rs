use std::array;

use bytes::BytesMut;

/// How many arenas the ring holds.
const ARENAS: usize = 4;
/// The bytes of an arena, unless a slot is larger: the ring reuses an arena once each datagram
/// of it is dropped, a frame after its read, so its three others cover a frame's 768 KiB, about
/// 46 MB/s at 60 Hz, before a read allocates. A datagram that lives longer, a message slice
/// Lightyear keeps, holds one arena.
const ARENA_BYTES: usize = 256 * 1024;

/// The memory the thread copies each read into, as `quinn-proto` takes a datagram by value: a
/// ring of arenas, each copy a view of the current one, so a datagram and the others of its read
/// share one allocation. A full arena gives way to the next, which the ring reuses once all its
/// views dropped, and replaces while one lives.
#[derive(Debug)]
pub(crate) struct ArenaRing {
    arenas: [BytesMut; ARENAS],
    current: usize,
    capacity: usize,
}

impl ArenaRing {
    /// A ring whose arenas each hold at least a slot of `slot` bytes.
    pub(crate) fn new(slot: usize) -> ArenaRing {
        let capacity = slot.max(ARENA_BYTES);
        ArenaRing {
            arenas: array::from_fn(|_| BytesMut::with_capacity(capacity)),
            current: 0,
            capacity,
        }
    }

    /// A copy of `bytes`, a view of the current arena, or of the next one when it lacks room.
    pub(crate) fn copy(&mut self, bytes: &[u8]) -> BytesMut {
        debug_assert!(
            bytes.len() <= self.capacity,
            "an arena holds at least a slot"
        );
        if self.arenas[self.current].capacity() < bytes.len() {
            self.current = (self.current + 1) % ARENAS;
            let arena = &mut self.arenas[self.current];
            if !arena.try_reclaim(self.capacity) {
                *arena = BytesMut::with_capacity(self.capacity);
            }
        }
        let arena = &mut self.arenas[self.current];
        arena.extend_from_slice(bytes);
        arena.split()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The start of the allocation `view` lies in: its first byte's address less its offset in
    /// its arena.
    fn base(view: &BytesMut, offset: usize) -> usize {
        view.as_ptr().addr() - offset
    }

    #[test]
    fn copies_share_an_arena_until_it_is_full_and_a_full_one_is_reused_once_its_views_drop() {
        let mut ring = ArenaRing::new(1024);
        assert_eq!(ring.capacity, ARENA_BYTES);
        // A slot larger than an arena sets the arena's size.
        assert_eq!(ArenaRing::new(ARENA_BYTES + 1).capacity, ARENA_BYTES + 1);

        // Three copies of 100 KiB: the first two fill arena 0 to 200 KiB; the third does not fit
        // its last 56 KiB, and starts arena 1.
        let chunk = vec![7_u8; 100 * 1024];
        let first = ring.copy(&chunk);
        let second = ring.copy(&chunk);
        assert_eq!(base(&second, chunk.len()), base(&first, 0));
        assert_eq!(*second, *chunk);
        let third = ring.copy(&chunk);
        assert_eq!(ring.current, 1);
        let arena_0 = base(&first, 0);
        assert_ne!(base(&third, 0), arena_0);

        // Arenas 1 to 3 fill, two copies each, and the ring comes back to arena 0. With its views
        // dropped, it is reused: the next copy starts at its first byte.
        drop((first, second));
        let mut held = vec![third];
        for _ in 0..5 {
            held.push(ring.copy(&chunk));
        }
        assert_eq!(ring.current, 3);
        let reused = ring.copy(&chunk);
        assert_eq!(ring.current, 0);
        assert_eq!(base(&reused, 0), arena_0);

        // Arena 1 is next, and a view of it lives: a new allocation takes its place.
        let arena_1 = base(&held[0], 0);
        drop(ring.copy(&chunk));
        let replaced = ring.copy(&chunk);
        assert_eq!(ring.current, 1);
        assert_ne!(base(&replaced, 0), arena_1);
        assert_eq!(*held[0], *chunk);
    }
}
