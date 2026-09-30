use serde::Deserialize;
use serde::de::value::{Error, StrDeserializer};

/// A stat, as data and `unit.stat(name)` name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stat {
    Health,
    HealthRegen,
    Resource,
    ResourceRegen,
    AttackDamage,
    AbilityPower,
    Armor,
    MagicResist,
    ArmorPen,
    MagicPen,
    MoveSpeed,
    PhysicalBlock,
    AttackDamagePct,
    AbilityPowerPct,
    AttackSpeedPct,
    MoveSpeedPct,
    HealingReceivedPct,
    DamageDealtPct,
    AttackSpeed,
    CritChance,
    ArmorPenPct,
    MagicPenPct,
    LifeSteal,
    SpellVamp,
    CooldownReduction,
    Slow,
}

impl Stat {
    /// The stat `name` names.
    pub fn named(name: &str) -> Option<Stat> {
        Stat::deserialize(StrDeserializer::<Error>::new(name)).ok()
    }
}
