use campfire_math::Num;
use campfire_sim::{Capability, StableId};

use crate::combat::damage_kind::DamageKind;
use crate::scripts::effects::Effect;
use crate::stats::pool_id::PoolId;

/// A change to units' pools that a call queued, from its acting unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CombatEffect {
    /// `amount` of `kind` damage to `target`, which joins the damage pass.
    Damage {
        target: StableId,
        amount: Num,
        kind: DamageKind,
    },
    /// A heal of `amount` to `unit`'s life pool, times one plus its `heal_scale` stat.
    Heal { unit: StableId, amount: Num },
    /// `amount` of `unit`'s `pool` back.
    Restore {
        unit: StableId,
        pool: PoolId,
        amount: Num,
    },
    /// An extra attack of the acting unit on `target`.
    AttackHit { target: StableId },
}

impl Effect for CombatEffect {
    const CAPABILITY: Capability = Capability::Combat;
}
