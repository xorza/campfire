use arrayvec::ArrayVec;
use blake3::Hasher;

use campfire_common::Sink;

/// A BLAKE3 hasher fed through a buffer. Postcard writes a value at a time, and from small pieces
/// BLAKE3 hashes one block at a time; from a piece of many chunks it hashes several at once with
/// SIMD. The buffer is its own, so a hash allocates nothing.
#[derive(Debug)]
pub(crate) struct HashSink {
    hasher: Hasher,
    buffer: ArrayVec<u8, { HashSink::FLUSH }>,
}

impl HashSink {
    /// The bytes it buffers before it hashes them: 16 chunks of 1 KiB, as many as the widest
    /// SIMD BLAKE3 hashes at once.
    const FLUSH: usize = 16 * 1024;

    pub(crate) fn new() -> HashSink {
        HashSink {
            hasher: Hasher::new(),
            buffer: ArrayVec::new_const(),
        }
    }

    /// The hash of the bytes it took since it was made or last finished, and a fresh start.
    pub(crate) fn finish(&mut self) -> [u8; 32] {
        self.hasher.update(&self.buffer);
        self.buffer.clear();
        let hash = *self.hasher.finalize().as_bytes();
        self.hasher.reset();
        hash
    }
}

impl Sink for HashSink {
    fn put(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let (taken, rest) = bytes.split_at(bytes.len().min(self.buffer.remaining_capacity()));
            self.buffer
                .try_extend_from_slice(taken)
                .expect("a piece within the buffer's room");
            bytes = rest;
            if self.buffer.is_full() {
                self.hasher.update(&self.buffer);
                self.buffer.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_sink_hashes_its_bytes_as_one_piece_does_and_starts_again_when_it_finishes() {
        // 40 000 bytes in pieces of 1 to 997, which cross the 16 KiB flush twice and leave a
        // tail: the hash of the whole in one piece. A finished sink hashes the next bytes alone.
        let bytes: Vec<u8> = (0..40_000_u32)
            .map(|at| u8::try_from(at * 7 % 251).unwrap())
            .collect();
        let mut sink = HashSink::new();
        let (mut at, mut piece) = (0, 1);
        while at < bytes.len() {
            let end = (at + piece).min(bytes.len());
            sink.put(&bytes[at..end]);
            (at, piece) = (end, piece * 31 % 997 + 1);
        }
        assert_eq!(sink.finish(), *blake3::hash(&bytes).as_bytes());
        // The same bytes in one piece, past two buffers' worth.
        sink.put(&bytes);
        assert_eq!(sink.finish(), *blake3::hash(&bytes).as_bytes());
        sink.put(b"next");
        assert_eq!(sink.finish(), *blake3::hash(b"next").as_bytes());
        assert_eq!(sink.finish(), *blake3::hash(b"").as_bytes());
    }
}
