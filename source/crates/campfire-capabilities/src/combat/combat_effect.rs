use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::StableId;

use crate::actions::effect_lists::Does;
use crate::combat::damage_pass::DamagePass;
use crate::scripts::effects::Effect;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::stats::pool_id::PoolId;
use crate::units::script_view::View;
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

impl CombatEffect {
    /// Queues the listed `does`, a damage, a heal or a restore, to `unit` in `frame`.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "its signature is the one every capability's listed effects queue by"
    )]
    pub(crate) fn queue_listed(
        does: Does,
        unit: StableId,
        frame: &mut Frame,
        _: &View,
    ) -> Result<(), CallError> {
        let effect = match does {
            Does::Damage { amount, kind } => CombatEffect::Damage {
                target: unit,
                amount: amount.number(frame),
                kind,
            },
            Does::Heal { amount } => CombatEffect::Heal {
                unit,
                amount: amount.number(frame),
            },
            Does::Restore { pool, amount } => CombatEffect::Restore {
                unit,
                pool,
                amount: amount.number(frame),
            },
            Does::Modifier { .. } | Does::Xp { .. } | Does::Purge { .. } | Does::Launch { .. } => {
                unreachable!("combat queues only its own listed effects")
            }
        };
        frame.effects.push(effect);
        Ok(())
    }
}

/// From the call's acting unit and its ability, at its depth of the chain of combat events.
impl Effect for CombatEffect {
    fn apply(self, world: &mut World, frame: &mut Frame, _: Tick) {
        DamagePass::apply_effect(world, self, frame);
    }
}
