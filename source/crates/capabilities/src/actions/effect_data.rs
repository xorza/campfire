use serde::de::{Error, IgnoredAny};
use serde::{Deserialize, Deserializer};

use crate::values::declared_name::DeclaredName;
use crate::values::number::Number;

/// One effect of an effect list, as data gives it: what it does, and to whom: the unit the list
/// reached, or with `to = "source"`, the acting unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectData {
    pub does: Effecting,
    pub to: EffectTo,
}

/// What an effect does: each the table of one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effecting {
    /// `damage = { amount, kind }`.
    Damage { amount: Number, kind: DeclaredName },
    /// `heal = { amount }`, of the life pool.
    Heal { amount: Number },
    /// `restore = { pool, amount }`.
    Restore { pool: DeclaredName, amount: Number },
    /// `modifier = { id, duration_ms }`, a modifier of the action's package, from the acting
    /// unit.
    Modifier {
        id: DeclaredName,
        duration_ms: Option<Number>,
    },
    /// `xp = { track, amount }`.
    Xp { track: DeclaredName, amount: Number },
    /// An effect the design names that the release does not run yet; the load refuses it.
    Planned(PlannedEffect),
}

/// The effects the design names that the release does not run yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedEffect {
    Purge,
    Spawn,
    Launch,
    Move,
    Loot,
    Noise,
}

/// Whom an effect applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EffectTo {
    /// The unit the list reached: a cast's target, or the unit a delivery hit.
    #[default]
    Reached,
    /// The acting unit.
    Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ToName {
    Source,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DamageFields {
    amount: Number,
    kind: DeclaredName,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HealFields {
    amount: Number,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreFields {
    pool: DeclaredName,
    amount: Number,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModifierFields {
    id: DeclaredName,
    duration_ms: Option<Number>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct XpFields {
    track: DeclaredName,
    amount: Number,
}

impl Effecting {
    /// The numbers it gives: its amount, and a modifier's duration.
    pub fn numbers(&self) -> impl Iterator<Item = &Number> + '_ {
        let (amount, duration) = match self {
            Effecting::Damage { amount, .. }
            | Effecting::Heal { amount }
            | Effecting::Restore { amount, .. }
            | Effecting::Xp { amount, .. } => (Some(amount), None),
            Effecting::Modifier { duration_ms, .. } => (None, duration_ms.as_ref()),
            Effecting::Planned(_) => (None, None),
        };
        amount.into_iter().chain(duration)
    }

    /// The modifier of its package it applies, if it applies one.
    pub fn modifier(&self) -> Option<&DeclaredName> {
        match self {
            Effecting::Modifier { id, .. } => Some(id),
            _ => None,
        }
    }
}

impl PlannedEffect {
    /// The effect's key in data.
    pub const fn name(self) -> &'static str {
        match self {
            PlannedEffect::Purge => "purge",
            PlannedEffect::Spawn => "spawn",
            PlannedEffect::Launch => "launch",
            PlannedEffect::Move => "move",
            PlannedEffect::Loot => "loot",
            PlannedEffect::Noise => "noise",
        }
    }
}

/// The table of exactly one effect's key, and `to` beside it.
impl<'de> Deserialize<'de> for EffectData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<EffectData, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            damage: Option<DamageFields>,
            heal: Option<HealFields>,
            restore: Option<RestoreFields>,
            modifier: Option<ModifierFields>,
            xp: Option<XpFields>,
            purge: Option<IgnoredAny>,
            spawn: Option<IgnoredAny>,
            launch: Option<IgnoredAny>,
            #[serde(rename = "move")]
            moves: Option<IgnoredAny>,
            loot: Option<IgnoredAny>,
            noise: Option<IgnoredAny>,
            to: Option<ToName>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let planned = [
            (fields.purge.is_some(), PlannedEffect::Purge),
            (fields.spawn.is_some(), PlannedEffect::Spawn),
            (fields.launch.is_some(), PlannedEffect::Launch),
            (fields.moves.is_some(), PlannedEffect::Move),
            (fields.loot.is_some(), PlannedEffect::Loot),
            (fields.noise.is_some(), PlannedEffect::Noise),
        ];
        let planned = planned
            .into_iter()
            .filter(|&(given, _)| given)
            .map(|(_, effect)| Effecting::Planned(effect));
        let damage = fields
            .damage
            .map(|DamageFields { amount, kind }| Effecting::Damage { amount, kind });
        let heal = fields
            .heal
            .map(|HealFields { amount }| Effecting::Heal { amount });
        let restore = fields
            .restore
            .map(|RestoreFields { pool, amount }| Effecting::Restore { pool, amount });
        let modifier = fields
            .modifier
            .map(|ModifierFields { id, duration_ms }| Effecting::Modifier { id, duration_ms });
        let xp = fields
            .xp
            .map(|XpFields { track, amount }| Effecting::Xp { track, amount });
        let mut effects = [damage, heal, restore, modifier, xp]
            .into_iter()
            .flatten()
            .chain(planned);
        let (Some(does), None) = (effects.next(), effects.next()) else {
            return Err(D::Error::custom("exactly one effect"));
        };
        let to = match fields.to {
            Some(ToName::Source) => EffectTo::Source,
            None => EffectTo::Reached,
        };
        Ok(EffectData { does, to })
    }
}
