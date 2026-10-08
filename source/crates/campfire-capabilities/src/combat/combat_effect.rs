use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::StableId;

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::capability_does::CapabilityDoes;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::damage_pass::DamagePass;
use crate::combat::heal::{Heal, HealCause};
use crate::combat::pass_queue::PassQueue;
use crate::scripts::effects::Effect;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::stats::pool_id::PoolId;
use crate::stats::unit_stats::UnitStats;
use crate::units::script_view::View;
use crate::values::damage_kind::DamageKind;
use campfire_sim::EntityIndex;

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
        does: CapabilityDoes,
        unit: StableId,
        _: Option<StableId>,
        frame: &mut Frame,
        _: &View,
    ) -> Result<(), CallError> {
        let effect = match does {
            CapabilityDoes::Damage { amount, kind } => CombatEffect::Damage {
                target: unit,
                amount: amount.number(frame),
                kind,
            },
            CapabilityDoes::Heal { amount } => CombatEffect::Heal {
                unit,
                amount: amount.number(frame),
            },
            CapabilityDoes::Restore { pool, amount } => CombatEffect::Restore {
                unit,
                pool,
                amount: amount.number(frame),
            },
            _ => unreachable!("combat queues only its own listed effects"),
        };
        frame.effects.push(effect);
        Ok(())
    }
}

/// From the call's acting unit and its ability, at its depth of the chain of combat events.
impl Effect for CombatEffect {
    /// Applies the effect, which the call in `frame` queued: from its acting unit, by its ability,
    /// at its chain depth, delivered by its hit. Damage and a heal join the pass's queue, a
    /// restore applies at once, and an extra attack queues the source's attack damage, when it
    /// still has an attack.
    fn apply(self, world: &mut World, frame: &mut Frame, _: Tick) {
        let (source, ability, depth) = (frame.acting(), frame.action(), frame.depth());
        let damage = |target, amount, kind, cause| Damage {
            source,
            target,
            amount,
            kind,
            cause,
            ability,
            depth,
            hit: frame.hit(),
        };
        match self {
            CombatEffect::Damage {
                target,
                amount,
                kind,
            } => {
                let damage = damage(target, amount, kind, DamageCause::Effect);
                world.resource_mut::<PassQueue>().push_damage(damage);
            }
            CombatEffect::Heal { unit, amount } => {
                world.resource_mut::<PassQueue>().push_heal(Heal {
                    source,
                    target: unit,
                    amount,
                    cause: HealCause::Effect,
                    ability,
                    depth,
                });
            }
            CombatEffect::Restore { unit, pool, amount } => {
                DamagePass::restore(world, unit, pool, amount);
            }
            CombatEffect::AttackHit { target } => {
                let index = world.resource::<EntityIndex>();
                let Some(unit) = source.and_then(|source| index.get(source)) else {
                    return;
                };
                let unit = world.entity(unit);
                let book = world.resource::<ActionBook>();
                let first = unit.get::<ActionSlots>().and_then(|slots| {
                    let slot = slots.slot(book.weapon_for(slots, None)?)?;
                    let weapon = book.get(slot.action?)?.kind.weapon()?;
                    Some((slot, slot.rank?, weapon))
                });
                let Some((slot, rank, weapon)) = first else {
                    return;
                };
                let stats = unit.get::<UnitStats>().map_or(&[][..], UnitStats::values);
                let hit = Damage {
                    ability: slot.action,
                    ..damage(
                        target,
                        weapon.damage(stats),
                        weapon.kind,
                        DamageCause::ExtraAttack { rank },
                    )
                };
                world.resource_mut::<PassQueue>().push_damage(hit);
            }
        }
    }
}
