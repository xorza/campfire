use campfire_script::ScriptFn;
use campfire_sim::StableId;
use campfire_values::ModifierId;

use crate::damage::Damage;

/// A combat event, which the modifiers that hear it answer by their scripts' hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatEvent {
    /// The interval of `carrier`'s instance of `id` from `source` comes.
    Interval {
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
    },
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

/// What answers each combat event: the hooks of the modifiers that hear it, which
/// `ModifierHooks` runs with the `ctx` ability scripts get.
pub type CombatEvents = ScriptFn<CombatEvent, ()>;
