use campfire_script::{Ctx, HookSpec, ScriptBatch, ScriptRole, ScriptValue};
use campfire_sim::StableId;
use campfire_stats::{ModifierHandle, ModifierHook, ModifierHooks};

use crate::damage::Damage;
use crate::damage_handle::DamageHandle;

/// A combat event, which the modifiers that hear it answer by their scripts' hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CombatEvent {
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

/// `on_attack(ctx, m, target)`: the modifier's carrier's attack goes off at `target`.
#[derive(Debug)]
pub(crate) struct OnAttack;

/// `on_attack_hit(ctx, m, d)`: the carrier's attack hits, with its damage.
#[derive(Debug)]
pub(crate) struct OnAttackHit;

/// `on_damage_taken(ctx, m, d)`: the carrier takes a damage.
#[derive(Debug)]
pub(crate) struct OnDamageTaken;

/// `on_kill(ctx, m, victim)`: the carrier kills `victim`.
#[derive(Debug)]
pub(crate) struct OnKill;

/// `on_takedown(ctx, m, victim)`: the carrier takes part in the kill of `victim`.
#[derive(Debug)]
pub(crate) struct OnTakedown;

/// A combat hook of the modifiers a unit holds, whose event gives a unit, `()` when gone, or a
/// damage.
macro_rules! combat_hook {
    ($($hook:ident: $name:literal, $param:literal, $event:ty;)*) => {$(
        impl HookSpec for $hook {
            const NAME: &'static str = $name;
            const ROLE: ScriptRole = ScriptRole::Modifier;
            const PARAMS: &'static [&'static str] = &["ctx", "m", $param];
            type Args = (Ctx, ModifierHandle, $event);
        }

        impl ModifierHook for $hook {
            type Event = $event;

            fn args(ctx: Ctx, m: ModifierHandle, event: $event) -> Self::Args {
                (ctx, m, event)
            }
        }
    )*};
}

combat_hook! {
    OnAttack: "on_attack", "target", ScriptValue;
    OnAttackHit: "on_attack_hit", "d", DamageHandle;
    OnDamageTaken: "on_damage_taken", "d", DamageHandle;
    OnKill: "on_kill", "victim", ScriptValue;
    OnTakedown: "on_takedown", "victim", ScriptValue;
}

impl CombatEvent {
    /// Answers the event by the hooks of the modifiers its unit holds that hear it, through
    /// `hooks`, the stats' runner of modifier scripts.
    pub(crate) fn reach(self, hooks: &ModifierHooks, batch: &mut ScriptBatch<'_>) {
        let unit = |id| hooks.view().unit_value(Some(id));
        let damage = |damage: Damage| DamageHandle::new(damage, hooks.view().clone());
        let next = |depth: u8| depth.saturating_add(1);
        match self {
            Self::Attack { attacker, target } => {
                hooks.run::<OnAttack>(batch, attacker, 1, || unit(target));
            }
            Self::AttackHit(hit) => {
                if let Some(attacker) = hit.source {
                    hooks.run::<OnAttackHit>(batch, attacker, next(hit.depth), || damage(hit));
                }
            }
            Self::DamageTaken(taken) => {
                let depth = next(taken.depth);
                hooks.run::<OnDamageTaken>(batch, taken.target, depth, || damage(taken));
            }
            Self::Kill {
                killer,
                victim,
                depth,
            } => hooks.run::<OnKill>(batch, killer, next(depth), || unit(victim)),
            Self::Takedown {
                unit: taker,
                victim,
                depth,
            } => hooks.run::<OnTakedown>(batch, taker, next(depth), || unit(victim)),
        }
    }
}
