use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionSlots, CapabilityDoesSpec};
use campfire_math::Num;
use campfire_script::{CallError, Effect, Frame, View};
use campfire_sim::{EntityIndex, StableId};
use campfire_stats::{Life, PoolId, UnitStats};
use campfire_values::DamageKind;

use crate::damage::{Damage, DamageCause};
use crate::damage_pass::DamagePass;
use crate::heal::{Heal, HealCause};
use crate::pass_queue::PassEntry;

/// A change to units' pools that a call queued, from its acting unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatEffect {
    /// `amount` of `kind` damage to `unit`, which joins the damage pass.
    Damage {
        unit: StableId,
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
    /// An extra attack of the acting unit on `unit`.
    AttackHit { unit: StableId },
    /// `unit`'s life taken whole, so it dies as the next Die stage finds it, with no killer.
    Kill { unit: StableId },
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
            CapabilityDoesSpec::Damage { amount, kind } => Self::Damage {
                unit: id,
                amount: amount.number(frame),
                kind,
            },
            CapabilityDoesSpec::Heal { amount } => Self::Heal {
                unit: id,
                amount: amount.number(frame),
            },
            CapabilityDoesSpec::Restore { pool, amount } => Self::Restore {
                unit: id,
                pool,
                amount: amount.number(frame),
            },
            _ => unreachable!("combat queues only its own listed effects"),
        };
        frame.queue(effect);
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
            Self::Damage { unit, amount, kind } => {
                let damage = damage(unit, amount, kind, DamageCause::Effect);
                DamagePass::queue(world, PassEntry::Damage(damage));
            }
            Self::Heal { unit, amount } => {
                let heal = Heal {
                    source,
                    target: unit,
                    amount,
                    cause: HealCause::Effect,
                    action,
                    depth,
                };
                DamagePass::queue(world, PassEntry::Heal(heal));
            }
            Self::Restore { unit, pool, amount } => {
                DamagePass::restore(world, unit, pool, amount);
            }
            Self::Kill { unit } => {
                if let Some(entity) = DamagePass::living(world, unit) {
                    let mut life = world.get_mut::<Life>(entity).expect("a living unit's life");
                    let all = life.current();
                    life.take(all);
                }
            }
            Self::AttackHit { unit: target } => {
                let index = world.resource::<EntityIndex>();
                let Some(unit) = source.and_then(|source| index.get(source)) else {
                    return;
                };
                let unit = world.entity(unit);
                let book = world.resource::<ActionBook>();
                let first = unit.get::<ActionSlots>().and_then(|slots| {
                    let slot = slots.slot(slots.weapon_in(book, None)?)?;
                    let weapon = book.get(slot.action?)?.kind.weapon()?;
                    let rank = slot.rank?;
                    Some((slot, rank, weapon, *book.get(slot.action?)?.values(rank)))
                });
                let Some((slot, rank, weapon, values)) = first else {
                    return;
                };
                let stats = unit.get::<UnitStats>().map_or(&[][..], UnitStats::values);
                let hit = Damage {
                    action: slot.action,
                    ..damage(
                        target,
                        weapon.damage(stats, &values),
                        weapon.kind,
                        DamageCause::ExtraAttack { rank },
                    )
                };
                DamagePass::queue(world, PassEntry::Damage(hit));
            }
        }
    }
}
