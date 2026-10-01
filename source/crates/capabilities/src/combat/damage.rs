use campfire_math::Num;
use campfire_sim::StableId;

use crate::abilities::ability_book::AbilityId;
use crate::combat::damage_kind::DamageKind;

/// A damage the pass deals: from its source, none from a modifier the mode applied, to its
/// target, its amount before `calc_damage`, its kind, what dealt it, the ability whose cast,
/// projectile, area or modifier dealt it, and the depth of the chain of events that dealt it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Damage {
    pub(crate) source: Option<StableId>,
    pub(crate) target: StableId,
    pub(crate) amount: Num,
    pub(crate) kind: DamageKind,
    pub(crate) cause: DamageCause,
    pub(crate) ability: Option<AbilityId>,
    pub(crate) depth: u8,
}

/// What dealt a damage: an attack, with the roll it drew as its windup ended; an extra attack,
/// from `ctx.attack_hit`, which draws none; or an ability's or modifier's effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DamageCause {
    Attack { roll: Num },
    ExtraAttack,
    Effect,
}

impl DamageCause {
    /// Whether an attack dealt it, an extra one included: `leech` heals by its `attack` stat
    /// from it, by its `other` stat from the rest.
    pub(crate) const fn attack(self) -> bool {
        matches!(self, DamageCause::Attack { .. } | DamageCause::ExtraAttack)
    }

    /// The roll of an attack, at least 0 and less than 1; `None` for any other cause.
    pub(crate) const fn roll(self) -> Option<Num> {
        match self {
            DamageCause::Attack { roll } => Some(roll),
            DamageCause::ExtraAttack | DamageCause::Effect => None,
        }
    }
}
