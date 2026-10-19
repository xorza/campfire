use bytes::BytesMut;

/// Memory that views are split off: a chunk filled from the front, each view sharing its
/// allocation, so many small buffers cost one allocation and a view never copies
/// (13-net-stack/packet-session.md, One `ByteArena` type, in `net-bytes`, holds every buffer that
/// views split off).
///
/// A chunk that lacks room gives way to the first spare whose views all dropped, or to a new
/// chunk; the chunk it leaves waits among the spares for its views, at most `max_spares` of them,
/// past which it goes. A chunk whose own views all dropped is reused in place first.
///
/// The bytes of the view being written stay at the chunk's front, after its earlier views, and
/// move whole to the next chunk when they outgrow this one; a view larger than a chunk gets an
/// allocation of its own.
#[derive(Debug)]
pub struct ByteArena {
    chunk: usize,
    max_spares: usize,
    /// The end of the current chunk: the view being written, then the room left.
    current: BytesMut,
    /// Whether `current` is the end of a chunk of `chunk` bytes, which can be reused.
    chunked: bool,
    /// The ends of earlier chunks, until the views split from them are dropped.
    spares: Vec<BytesMut>,
    /// The chunks and the views of their hold the arena allocated.
    #[cfg(feature = "internals")]
    allocations: usize,
}

impl ByteArena {
    /// An arena of chunks of `chunk` bytes, which keeps at most `max_spares` of them for reuse
    /// besides the current one. It allocates nothing before its first view.
    pub fn new(chunk: usize, max_spares: usize) -> Self {
        Self {
            chunk,
            max_spares,
            current: BytesMut::new(),
            chunked: false,
            spares: Vec::new(),
            #[cfg(feature = "internals")]
            allocations: 0,
        }
    }

    /// The arena with the first view in an allocation of its own of `capacity` bytes, which
    /// grows as a plain `BytesMut` does: for a one-off writer, which needs no chunk.
    #[must_use]
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.current = BytesMut::with_capacity(capacity);
        self.chunked = false;
        self
    }

    /// An arena whose current chunk and `max_spares` spares are allocated now, so no later view
    /// pays for them.
    pub fn preallocated(chunk: usize, max_spares: usize) -> Self {
        let mut arena = Self::new(chunk, max_spares);
        arena.current = arena.new_chunk();
        arena.chunked = true;
        while arena.spares.len() < max_spares {
            let spare = arena.new_chunk();
            arena.spares.push(spare);
        }
        arena
    }

    /// The bytes of a chunk.
    pub const fn chunk(&self) -> usize {
        self.chunk
    }

    /// Sets the bytes of a chunk, dropping the kept chunks when it changes: each later view comes
    /// from a chunk of the new size.
    pub fn set_chunk(&mut self, chunk: usize) {
        if self.chunk == chunk {
            return;
        }
        self.chunk = chunk;
        self.spares.clear();
        if self.chunked {
            let written = self.current.split();
            self.current = written;
            self.chunked = false;
        }
    }

    /// The bytes of the view being written.
    pub fn len(&self) -> usize {
        self.current.len()
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_empty()
    }

    /// The view being written, with room for `additional` more bytes in its chunk: a write
    /// within that room neither allocates nor copies.
    pub fn reserve(&mut self, additional: usize) -> &mut BytesMut {
        if self.current.capacity() - self.current.len() < additional {
            self.make_room(additional);
        }
        &mut self.current
    }

    /// Appends `bytes` to the view being written.
    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.reserve(bytes.len()).extend_from_slice(bytes);
    }

    /// Splits the view written so far off the arena. O(1): it shares the chunk.
    pub fn split(&mut self) -> BytesMut {
        self.current.split()
    }

    /// A view holding a copy of `bytes`.
    pub fn copy(&mut self, bytes: &[u8]) -> BytesMut {
        self.extend_from_slice(bytes);
        self.split()
    }

    /// An empty view with room for `capacity` bytes, which a write within it fills in place.
    pub fn take(&mut self, capacity: usize) -> BytesMut {
        debug_assert!(self.is_empty(), "a view is taken between writes");
        self.reserve(capacity);
        let rest = self.current.split_off(capacity);
        core::mem::replace(&mut self.current, rest)
    }

    /// Drops the view being written and keeps its room.
    pub fn reset(&mut self) {
        self.current.clear();
    }

    /// The view being written, as the arena gives it up.
    pub fn into_written(self) -> BytesMut {
        self.current
    }

    /// Makes room for `additional` more bytes of the view being written: in the current chunk
    /// once its earlier views are dropped, else in a spare or a new chunk.
    fn make_room(&mut self, additional: usize) {
        if self.chunked && self.current.try_reclaim(additional) {
            return;
        }
        let needed = self.current.len() + additional;
        if needed > self.chunk {
            self.current.reserve(additional);
            self.chunked = false;
            #[cfg(feature = "internals")]
            {
                self.allocations += 1;
            }
            return;
        }
        let first = !core::mem::replace(&mut self.chunked, true);
        let mut chunk = if first && self.spares.len() < self.max_spares {
            // A writer whose views live past the next chunk switch needs a second chunk: made
            // with the first, so no later view pays for it.
            let spare = self.new_chunk();
            self.spares.push(spare);
            self.new_chunk()
        } else {
            self.reclaimed_chunk().unwrap_or_else(|| self.new_chunk())
        };
        chunk.extend_from_slice(&self.current);
        let mut retired = core::mem::replace(&mut self.current, chunk);
        if !first && self.spares.len() < self.max_spares {
            retired.clear();
            self.spares.push(retired);
        }
    }

    /// A spare chunk whose views are all dropped, made whole again.
    fn reclaimed_chunk(&mut self) -> Option<BytesMut> {
        let chunk = self.chunk;
        let index = self
            .spares
            .iter_mut()
            .position(|spare| spare.try_reclaim(chunk))?;
        Some(self.spares.swap_remove(index))
    }

    /// An empty chunk that already shares its allocation, so splitting views off it allocates
    /// nothing more.
    #[cfg_attr(
        not(feature = "internals"),
        expect(
            clippy::needless_pass_by_ref_mut,
            reason = "the allocation count of `internals` takes it"
        )
    )]
    fn new_chunk(&mut self) -> BytesMut {
        #[cfg(feature = "internals")]
        {
            self.allocations += 1;
        }
        let mut chunk = BytesMut::with_capacity(self.chunk);
        drop(chunk.split());
        chunk
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::byte_arena::ByteArena;

    impl ByteArena {
        /// The chunks and the views of their hold the arena allocated.
        pub const fn allocations(&self) -> usize {
            self.allocations
        }

        /// The chunks the arena keeps for reuse besides the current one.
        pub const fn spares(&self) -> usize {
            self.spares.len()
        }
    }
}

#[cfg(test)]
mod tests;
