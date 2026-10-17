use campfire_script::{Hook, ScriptBatch, ScriptValue};
use campfire_sim::StableId;
use campfire_stats::ModifierHooks;

use crate::damage::Damage;
use crate::damage_handle::DamageHandle;

/// A combat event, which the modifiers that hear it answer by their scripts' hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatEvent {
    /// `attacker`'s attack on `target` goes off, as its windup ends.
    Attack {
        attacker: StableId,
        target: StableId,
    },
    /// An attack's damage hit, its amount after `calc_damage`.
    AttackHit(Damage),
    /// A damage was taken, its amount after `calc_damage`.
    DamageTaken(Damage),
    /// `killer` killed `victim` with a damage at chain depth `depth`.
    Kill {
        killer: StableId,
        victim: StableId,
        depth: u8,
    },
    /// `unit`, the killer or an assister, took part in the kill of `victim`.
    Takedown {
        unit: StableId,
        victim: StableId,
        depth: u8,
    },
}

impl CombatEvent {
    /// Answers the event by the hooks of the modifiers its unit holds that hear it, through
    /// `hooks`, the stats' runner of modifier scripts.
    pub(crate) fn reach(self, hooks: &ModifierHooks, batch: &mut ScriptBatch<'_>) {
        let unit = |id| Some(hooks.view().unit_value(Some(id)));
        let damage = |damage: Damage| {
            let handle = DamageHandle::new(damage, hooks.view().clone());
            Some(ScriptValue::of(handle))
        };
        let next = |depth: u8| depth.saturating_add(1);
        match self {
            Self::Attack { attacker, target } => {
                hooks.run(batch, attacker, Hook::OnAttack, 1, || unit(target));
            }
            Self::AttackHit(hit) => {
                if let Some(attacker) = hit.source {
                    hooks.run(batch, attacker, Hook::OnAttackHit, next(hit.depth), || {
                        damage(hit)
                    });
                }
            }
            Self::DamageTaken(taken) => {
                let depth = next(taken.depth);
                hooks.run(batch, taken.target, Hook::OnDamageTaken, depth, || {
                    damage(taken)
                });
            }
            Self::Kill {
                killer,
                victim,
                depth,
            } => hooks.run(batch, killer, Hook::OnKill, next(depth), || unit(victim)),
            Self::Takedown {
                unit: taker,
                victim,
                depth,
            } => hooks.run(batch, taker, Hook::OnTakedown, next(depth), || unit(victim)),
        }
    }
}
