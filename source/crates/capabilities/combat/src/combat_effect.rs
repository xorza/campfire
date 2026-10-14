use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionSlots, CapabilityDoesSpec};
use campfire_math::Num;
use campfire_script::{CallError, Effect, Frame, View};
use campfire_sim::{EntityIndex, StableId};
use campfire_stats::{PoolId, UnitStats};
use campfire_values::DamageKind;

use crate::damage::{Damage, DamageCause};
use crate::damage_pass::DamagePass;
use crate::heal::{Heal, HealCause};
use crate::pass_queue::PassQueue;

/// A change to units' pools that a call queued, from its acting unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatEffect {
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
        id: StableId,
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
        does: CapabilityDoesSpec,
        id: StableId,
        _: Option<StableId>,
        frame: &mut Frame,
        _: &View,
    ) -> Result<(), CallError> {
        let effect = match does {
            CapabilityDoesSpec::Damage { amount, kind } => CombatEffect::Damage {
                target: id,
                amount: amount.number(frame),
                kind,
            },
            CapabilityDoesSpec::Heal { amount } => CombatEffect::Heal {
                unit: id,
                amount: amount.number(frame),
            },
            CapabilityDoesSpec::Restore { pool, amount } => CombatEffect::Restore {
                id,
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
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let (source, action, depth) = (frame.acting(), frame.action(), frame.depth());
        let damage = |target, amount, kind, cause| Damage {
            source,
            target,
            amount,
            kind,
            cause,
            action,
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
                    action,
                    depth,
                });
            }
            CombatEffect::Restore { id, pool, amount } => {
                DamagePass::restore(world, id, pool, amount);
            }
            CombatEffect::AttackHit { target } => {
                let index = world.resource::<EntityIndex>();
                let Some(unit) = source.and_then(|source| index.get(source)) else {
                    return;
                };
                let unit = world.entity(unit);
                let book = world.resource::<ActionBook>();
                let first = unit.get::<ActionSlots>().and_then(|slots| {
                    let slot = slots.slot(slots.weapon_in(book, None)?)?;
                    let weapon = book.get(slot.action?)?.kind.weapon()?;
                    Some((slot, slot.rank?, weapon))
                });
                let Some((slot, rank, weapon)) = first else {
                    return;
                };
                let stats = unit.get::<UnitStats>().map_or(&[][..], UnitStats::values);
                let hit = Damage {
                    action: slot.action,
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
