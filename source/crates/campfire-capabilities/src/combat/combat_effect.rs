use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::StableId;

use crate::combat::damage_pass::DamagePass;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::stats::pool_id::PoolId;
use crate::values::damage_kind::DamageKind;

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

/// From the call's acting unit and its ability, at its depth of the chain of combat events.
impl Effect for CombatEffect {
    fn apply(self, world: &mut World, frame: &mut Frame, _: Tick) {
        DamagePass::apply_effect(world, self, frame);
    }
}
