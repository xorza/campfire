use campfire_math::Num;
use campfire_sim::StableId;
use campfire_values::{ActionId, DamageKind, Hit, Rank};

/// A damage the pass deals: from its source, none from a modifier the mode applied, to its
/// target, its amount before `calc_damage`, its kind, what dealt it, the ability whose cast,
/// projectile, area or modifier dealt it, the depth of the chain of events that dealt it, and how
/// its delivery reached the target, none for damage no projectile or area delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Damage {
    pub source: Option<StableId>,
    pub target: StableId,
    pub amount: Num,
    pub kind: DamageKind,
    pub cause: DamageCause,
    pub action: Option<ActionId>,
    pub depth: u8,
    pub hit: Option<Hit>,
}

/// What dealt a damage: an attack, with the roll it drew as its windup ended; an extra attack,
/// from `ctx.attack_hit`, which draws none; or an ability's or modifier's effect. An attack names
/// the rank of its weapon's slot, at which the weapon's `on_hit` list reads its params.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageCause {
    Attack { roll: Num, rank: Rank },
    ExtraAttack { rank: Rank },
    Effect,
}

impl DamageCause {
    /// Whether an attack dealt it, an extra one included: `leech` heals by its `attack` stat
    /// from it, by its `other` stat from the rest.
    pub(crate) const fn attack(self) -> bool {
        self.weapon_rank().is_some()
    }

    /// The roll of an attack, at least 0 and less than 1; `None` for any other cause.
    pub const fn roll(self) -> Option<Num> {
        match self {
            Self::Attack { roll, .. } => Some(roll),
            Self::ExtraAttack { .. } | Self::Effect => None,
        }
    }

    /// The rank of the weapon an attack dealt it with, an extra one included; `None` for an
    /// effect.
    pub(crate) const fn weapon_rank(self) -> Option<Rank> {
        match self {
            Self::Attack { rank, .. } | Self::ExtraAttack { rank } => Some(rank),
            Self::Effect => None,
        }
    }
}
