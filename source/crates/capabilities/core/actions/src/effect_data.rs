use campfire_script::{DataSection, DataTable};
use campfire_values::{DeclaredName, Number};
use serde::de::{Error, IgnoredAny};
use serde::{Deserialize, Deserializer};

/// One effect of an effect list, as data gives it: what it does, and to whom: the unit the list
/// reached, or with `to = "source"`, the acting unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectData {
    pub does: DoesData,
    pub to: EffectTo,
}

/// What an effect does: each the table of one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoesData {
    /// `damage = { amount, kind }`.
    Damage(DamageFields),
    /// `heal = { amount }`, of the life pool.
    Heal(HealFields),
    /// `restore = { pool, amount }`.
    Restore(RestoreFields),
    /// `modifier = { id, duration_ms }`, a modifier of the action's package, from the acting
    /// unit.
    Modifier(ModifierFields),
    /// `xp = { track, amount }`.
    Xp(XpFields),
    /// `purge = { tag }`, of the modifiers that grant the tag.
    Purge(PurgeFields),
    /// `launch = { area, on_hit, on_end }`: an area type of the action's package, which runs its
    /// own lists.
    Launch(LaunchFields),
    /// `move = { to, speed }` or `move = { from, distance, ms }`: a forced move.
    Move(MoveData),
    /// `spawn = { unit_type, duration_ms }`: a unit of the mode's type `unit_type` where the
    /// effect applies, despawning `duration_ms` after it spawns when that is given.
    Spawn(SpawnFields),
    /// An effect the design names that the release does not run yet; the load refuses it.
    Planned(PlannedEffect),
}

/// A forced move of the unit an effect applies to, whose other unit is `"source"`, the acting
/// unit, or `"reached"`, the unit the list reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveData {
    /// A dash at `speed` meters a second to `to`, which it follows until their bodies touch.
    Dash { to: EffectTo, speed: Number },
    /// A knock back `distance` away from `from` over `ms`.
    KnockBack {
        from: EffectTo,
        distance: Number,
        ms: Number,
    },
}

campfire_common::enum_all! {
    /// The effects the design names that the release does not run yet.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PlannedEffect {
        Loot,
        Noise,
    }
    pub const ALL;
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

/// A forced move's other unit, as data names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MoveUnit {
    Source,
    Reached,
}

impl MoveUnit {
    const fn to(self) -> EffectTo {
        match self {
            MoveUnit::Source => EffectTo::Source,
            MoveUnit::Reached => EffectTo::Reached,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageFields {
    pub amount: Number,
    pub kind: DeclaredName,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealFields {
    pub amount: Number,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreFields {
    pub pool: DeclaredName,
    pub amount: Number,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModifierFields {
    pub id: DeclaredName,
    pub duration_ms: Option<Number>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XpFields {
    pub track: DeclaredName,
    pub amount: Number,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeFields {
    pub tag: DeclaredName,
}

/// A dash's `to` and `speed`, or a knock back's `from`, `distance` and `ms`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveFields {
    to: Option<MoveUnit>,
    speed: Option<Number>,
    from: Option<MoveUnit>,
    distance: Option<Number>,
    ms: Option<Number>,
}

impl MoveFields {
    fn data(self) -> Option<MoveData> {
        match self {
            MoveFields {
                to: Some(to),
                speed: Some(speed),
                from: None,
                distance: None,
                ms: None,
            } => Some(MoveData::Dash { to: to.to(), speed }),
            MoveFields {
                to: None,
                speed: None,
                from: Some(from),
                distance: Some(distance),
                ms: Some(ms),
            } => Some(MoveData::KnockBack {
                from: from.to(),
                distance,
                ms,
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnFields {
    pub unit_type: DeclaredName,
    pub duration_ms: Option<Number>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchFields {
    pub area: DeclaredName,
    #[serde(default)]
    pub on_hit: Vec<EffectData>,
    #[serde(default)]
    pub on_end: Vec<EffectData>,
}

impl DoesData {
    /// The numbers it gives: its amount, a modifier's duration, a dash's speed, or a knock
    /// back's distance and time.
    pub fn numbers(&self) -> impl Iterator<Item = &Number> + '_ {
        let numbers = match self {
            DoesData::Damage(DamageFields { amount, .. })
            | DoesData::Heal(HealFields { amount })
            | DoesData::Restore(RestoreFields { amount, .. })
            | DoesData::Xp(XpFields { amount, .. })
            | DoesData::Move(MoveData::Dash { speed: amount, .. }) => [Some(amount), None],
            DoesData::Modifier(ModifierFields { duration_ms, .. })
            | DoesData::Spawn(SpawnFields { duration_ms, .. }) => [duration_ms.as_ref(), None],
            DoesData::Move(MoveData::KnockBack { distance, ms, .. }) => [Some(distance), Some(ms)],
            DoesData::Purge(PurgeFields { .. })
            | DoesData::Launch(LaunchFields { .. })
            | DoesData::Planned(_) => [None, None],
        };
        numbers.into_iter().flatten()
    }

    /// The effects of the lists it holds: a launch's `on_hit`, then its `on_end`.
    pub fn nested(&self) -> impl Iterator<Item = &EffectData> + '_ {
        let (on_hit, on_end): (&[EffectData], &[EffectData]) = match self {
            DoesData::Launch(LaunchFields { on_hit, on_end, .. }) => (on_hit, on_end),
            _ => (&[], &[]),
        };
        on_hit.iter().chain(on_end)
    }

    /// The modifier of its package it applies, if it applies one.
    pub const fn modifier(&self) -> Option<&DeclaredName> {
        match self {
            DoesData::Modifier(ModifierFields { id, .. }) => Some(id),
            _ => None,
        }
    }
}

impl EffectData {
    /// The keys of an effect's table the release runs: each effect's that runs, and `to`, where
    /// it goes; the planned effects' are `PlannedEffect`'s.
    pub const KEYS: [&'static str; 10] = [
        "damage", "heal", "restore", "modifier", "xp", "purge", "launch", "move", "spawn", "to",
    ];
}

impl PlannedEffect {
    /// The effect's key in data.
    pub const fn name(self) -> &'static str {
        match self {
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
            purge: Option<PurgeFields>,
            spawn: Option<SpawnFields>,
            launch: Option<LaunchFields>,
            #[serde(rename = "move")]
            moves: Option<MoveFields>,
            loot: Option<IgnoredAny>,
            noise: Option<IgnoredAny>,
            to: Option<ToName>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let planned = [
            (fields.loot.is_some(), PlannedEffect::Loot),
            (fields.noise.is_some(), PlannedEffect::Noise),
        ];
        let planned = planned
            .into_iter()
            .filter(|&(given, _)| given)
            .map(|(_, effect)| DoesData::Planned(effect));
        let damage = fields.damage.map(DoesData::Damage);
        let heal = fields.heal.map(DoesData::Heal);
        let restore = fields.restore.map(DoesData::Restore);
        let modifier = fields.modifier.map(DoesData::Modifier);
        let xp = fields.xp.map(DoesData::Xp);
        let purge = fields.purge.map(DoesData::Purge);
        let launch = fields.launch.map(DoesData::Launch);
        let moves = match fields.moves.map(MoveFields::data) {
            Some(None) => {
                return Err(D::Error::custom(
                    "a dash's `to` and `speed`, or a knock back's `from`, `distance` and `ms`",
                ));
            }
            moves => moves.flatten().map(DoesData::Move),
        };
        let spawn = fields.spawn.map(DoesData::Spawn);
        let mut effects = [
            damage, heal, restore, modifier, xp, purge, launch, moves, spawn,
        ]
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

impl DataSection for EffectData {
    const TABLE: DataTable = DataTable::Effect;
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use crate::effect_data::EffectData;

    #[test]
    fn each_key_the_release_runs_is_one_an_effect_reads() {
        // An empty table under a key the effect reads lacks that effect's fields, or is no
        // effect at all for `to`; under any other key, it is an unknown field.
        let refusal = |key: &str| {
            let text = format!("{key} = {{}}");
            Toml::parse::<EffectData>(&text)
                .unwrap_err()
                .message()
                .to_owned()
        };
        for key in EffectData::KEYS {
            let message = refusal(key);
            assert!(!message.contains("unknown field"), "{key}: {message}");
        }
        assert!(refusal("bogus").contains("unknown field"));
    }
}
