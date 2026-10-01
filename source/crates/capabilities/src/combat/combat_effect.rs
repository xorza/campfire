use campfire_math::Num;
use campfire_sim::StableId;

use crate::combat::damage_kind::DamageKind;

/// A change to units' health or resource that a call queued, from its acting unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CombatEffect {
    /// `amount` of `kind` damage to `target`, which joins the damage pass.
    Damage {
        target: StableId,
        amount: Num,
        kind: DamageKind,
    },
    /// A heal of `amount` to `unit`, scaled by its `healing_received_pct`.
    Heal { unit: StableId, amount: Num },
    /// `amount` of `unit`'s resource back.
    Restore { unit: StableId, amount: Num },
    /// An extra attack of the acting unit on `target`.
    AttackHit { target: StableId },
}
