use campfire_math::PlayerSlot;
use campfire_sim::{Capability, StableId, Ticks};

use crate::scripts::effects::Effect;
use crate::stats::modifier_book::ModifierId;

/// A change to a unit's modifiers that a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModifierEffect {
    /// Modifier `id` on `target`, for `duration` when the call names one.
    Add {
        target: StableId,
        id: ModifierId,
        duration: Option<Ticks>,
    },
    /// Modifier `id` held by `player`, for the units it owns.
    AddPlayer { player: PlayerSlot, id: ModifierId },
    /// The end of the instance of `id` from `source` on `carrier`.
    Remove {
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
    },
}

impl Effect for ModifierEffect {
    const CAPABILITY: Capability = Capability::Stats;
}
