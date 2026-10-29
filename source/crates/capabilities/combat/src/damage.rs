use bevy_ecs::world::World;
use campfire_actions::ActionBook;
use campfire_math::Num;
use campfire_names::MatchNames;
use campfire_sim::StableId;
use campfire_values::{ActionId, Allegiance, DamageKind, Hit, Rank};
use serde::{Deserialize, Serialize};

/// A damage the pass deals: from its source, none from a modifier the mode applied, to its
/// target, its amount before `calc_damage`, its kind, what dealt it, the ability whose cast,
/// projectile, area or modifier dealt it, the depth of the chain of events that dealt it, and how
/// its delivery reached the target, none for damage no projectile or area delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
/// from `ctx.attack_hit`, which draws none; an ability's or modifier's effect; or the upkeep of a
/// capability above combat, as an airfield's toll on a unit it cannot take in. An attack names
/// the rank of its weapon's slot, at which the weapon's `on_hit` list reads its params, and its
/// attacker's allegiance as it struck, which the list acts for once the attacker is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageCause {
    Attack {
        roll: Num,
        rank: Rank,
        allegiance: Allegiance,
    },
    ExtraAttack {
        rank: Rank,
        allegiance: Allegiance,
    },
    Effect,
    Upkeep,
}

/// The weapon an attack dealt a damage with: its slot's rank, and the attacker's allegiance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WeaponHit {
    pub(crate) rank: Rank,
    pub(crate) allegiance: Allegiance,
}

impl Damage {
    /// Whether a restored damage holds what the pass reads: an action the book holds, if it names
    /// one, a kind the mode declares, which `calc_damage` reads by its name, and an attacker's
    /// allegiance of the match.
    pub(crate) fn is_known(&self, world: &World) -> bool {
        let book = world.resource::<ActionBook>();
        let names = world.get_resource::<MatchNames>();
        let allegiance = self.cause.weapon_hit().map(|hit| hit.allegiance);
        self.action.is_none_or(|action| book.get(action).is_some())
            && names.is_some_and(|names| {
                names.has_damage_kind(self.kind)
                    && allegiance.is_none_or(|allegiance| names.has_allegiance(allegiance))
            })
    }
}

impl DamageCause {
    /// Whether an attack dealt it, an extra one included: `leech` heals by its `attack` stat
    /// from it, by its `other` stat from the rest.
    pub(crate) const fn is_attack(self) -> bool {
        self.weapon_hit().is_some()
    }

    /// The roll of an attack, at least 0 and less than 1; `None` for any other cause.
    pub const fn roll(self) -> Option<Num> {
        match self {
            Self::Attack { roll, .. } => Some(roll),
            Self::ExtraAttack { .. } | Self::Effect | Self::Upkeep => None,
        }
    }

    /// The weapon an attack dealt it with, an extra one included; `None` for an effect.
    pub(crate) const fn weapon_hit(self) -> Option<WeaponHit> {
        match self {
            Self::Attack {
                rank, allegiance, ..
            }
            | Self::ExtraAttack { rank, allegiance } => Some(WeaponHit { rank, allegiance }),
            Self::Effect | Self::Upkeep => None,
        }
    }
}
