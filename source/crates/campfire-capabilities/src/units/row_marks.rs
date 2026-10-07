use bevy_ecs::entity::Entity;

/// Which sources of the script view must fill a unit's row again in the running read, as their
/// parts of it changed: a bit for each source, by the unit's entity index.
#[derive(Debug, Default)]
pub(crate) struct RowMarks {
    bits: Vec<u32>,
    /// The entity index of each unit marked, to clear.
    marked: Vec<u32>,
}

impl RowMarks {
    /// The sources a unit's marks have room for.
    pub(crate) const SOURCES: usize = u32::BITS as usize;

    /// Marks `entity`'s row for `source` to fill again.
    pub(crate) fn mark(&mut self, entity: Entity, source: usize) {
        debug_assert!(source < RowMarks::SOURCES);
        let at = entity.index_u32();
        let index = at as usize;
        if self.bits.len() <= index {
            self.bits.resize(index + 1, 0);
        }
        if self.bits[index] == 0 {
            self.marked.push(at);
        }
        self.bits[index] |= 1 << source;
    }

    /// Whether `entity`'s row is marked for `source`.
    pub(crate) fn marked(&self, entity: Entity, source: usize) -> bool {
        let bits = self.bits.get(entity.index_u32() as usize).copied();
        bits.unwrap_or(0) & 1 << source != 0
    }

    /// Whether `entity`'s row is marked for any source.
    pub(crate) fn any(&self, entity: Entity) -> bool {
        self.bits
            .get(entity.index_u32() as usize)
            .is_some_and(|&bits| bits != 0)
    }

    /// Clears every mark.
    pub(crate) fn clear(&mut self) {
        for &at in &self.marked {
            self.bits[at as usize] = 0;
        }
        self.marked.clear();
    }
}
