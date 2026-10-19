use campfire_net_bytes::ByteArena;

/// How many chunks the arena holds.
const CHUNKS: usize = 4;
/// The bytes of a chunk, unless a slot is larger: the arena reuses a chunk once each datagram
/// of it is dropped, a frame after its read, so its three others cover a frame's 768 KiB, about
/// 46 MB/s at 60 Hz, before a read allocates. A datagram that lives longer, a message slice
/// the stack keeps, holds one chunk.
const CHUNK_BYTES: usize = 256 * 1024;

/// The memory an IO copies each datagram it reads into, as a packet session opens a datagram in
/// place and the frame then keeps a message slice of it: each copy a view of a chunk, so a datagram
/// and the others of its read share one allocation, and a slice of one never promotes. Each chunk
/// holds at least a slot of `slot` bytes, and all are allocated now
/// (13-net-stack/packet-session.md, An IO copies each datagram it reads into an arena of four
/// chunks).
pub fn datagram_arena(slot: usize) -> ByteArena {
    ByteArena::preallocated(slot.max(CHUNK_BYTES), CHUNKS - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A slot larger than a chunk sets the chunk's size, and the arena holds its four chunks
    /// from the start.
    #[test]
    fn a_datagram_arena_holds_four_chunks_of_at_least_a_slot() {
        assert_eq!(datagram_arena(1024).chunk(), CHUNK_BYTES);
        assert_eq!(datagram_arena(CHUNK_BYTES + 1).chunk(), CHUNK_BYTES + 1);
        assert_eq!(datagram_arena(1024).allocations(), CHUNKS);
    }
}
