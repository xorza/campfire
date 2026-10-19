use bevy_ecs::entity::Entity;

/// Which sources of the script view must fill a unit's row again in the running read, as their
/// parts of it changed: a bit for each source, by the unit's entity index.
#[derive(Debug, Default)]
pub(crate) struct RowMarks {
    bits: Vec<u32>,
    /// Each unit marked, to visit and to clear.
    marked: Vec<Entity>,
}

impl RowMarks {
    /// The sources a unit's marks have room for.
    pub(crate) const SOURCES: usize = u32::BITS as usize;

    /// Marks `entity`'s row for `source` to fill again.
    pub(crate) fn mark(&mut self, entity: Entity, source: usize) {
        debug_assert!(source < Self::SOURCES);
        let at = entity.index_u32();
        let index = at as usize;
        if self.bits.len() <= index {
            self.bits.resize(index + 1, 0);
        }
        if self.bits[index] == 0 {
            self.marked.push(entity);
        }
        self.bits[index] |= 1 << source;
    }

    /// Whether `entity`'s row is marked for `source`.
    pub(crate) fn marked(&self, entity: Entity, source: usize) -> bool {
        let bits = self.bits.get(entity.index_u32() as usize).copied();
        bits.unwrap_or(0) & (1 << source) != 0
    }

    /// Each unit marked for any source, in the order first marked.
    pub(crate) fn entities(&self) -> &[Entity] {
        &self.marked
    }

    /// Clears every mark.
    pub(crate) fn clear(&mut self) {
        for &entity in &self.marked {
            self.bits[entity.index_u32() as usize] = 0;
        }
        self.marked.clear();
    }
}
