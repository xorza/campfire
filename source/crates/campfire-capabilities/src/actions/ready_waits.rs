use bevy_ecs::entity::Entity;

/// The units that wait for a passive that holds only while its action is off cooldown, by entity
/// index: time alone, with no change to the unit, makes such a passive hold.
#[derive(Debug, Default)]
pub(crate) struct ReadyWaits {
    bits: Vec<u64>,
}

impl ReadyWaits {
    pub(crate) fn waits(&self, entity: Entity) -> bool {
        let at = entity.index_u32() as usize;
        self.bits
            .get(at / 64)
            .is_some_and(|word| word & 1 << (at % 64) != 0)
    }

    /// Notes whether `entity` waits.
    pub(crate) fn set(&mut self, entity: Entity, waits: bool) {
        let at = entity.index_u32() as usize;
        if self.bits.len() <= at / 64 {
            if !waits {
                return;
            }
            self.bits.resize(at / 64 + 1, 0);
        }
        let bit = 1 << (at % 64);
        if waits {
            self.bits[at / 64] |= bit;
        } else {
            self.bits[at / 64] &= !bit;
        }
    }
}
