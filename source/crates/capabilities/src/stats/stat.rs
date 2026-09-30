use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A stat, as data and `unit.stat(name)` name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    pub const ALL: [Stat; 26] = [
        Stat::Health,
        Stat::HealthRegen,
        Stat::Resource,
        Stat::ResourceRegen,
        Stat::AttackDamage,
        Stat::AbilityPower,
        Stat::Armor,
        Stat::MagicResist,
        Stat::ArmorPen,
        Stat::MagicPen,
        Stat::MoveSpeed,
        Stat::PhysicalBlock,
        Stat::AttackDamagePct,
        Stat::AbilityPowerPct,
        Stat::AttackSpeedPct,
        Stat::MoveSpeedPct,
        Stat::HealingReceivedPct,
        Stat::DamageDealtPct,
        Stat::AttackSpeed,
        Stat::CritChance,
        Stat::ArmorPenPct,
        Stat::MagicPenPct,
        Stat::LifeSteal,
        Stat::SpellVamp,
        Stat::CooldownReduction,
        Stat::Slow,
    ];

    /// The stat named `name`.
    pub fn named(name: &str) -> Option<Stat> {
        Stat::ALL.into_iter().find(|stat| stat.name() == name)
    }

    /// The stat as data and scripts name it.
    pub const fn name(self) -> &'static str {
        match self {
            Stat::Health => "health",
            Stat::HealthRegen => "health_regen",
            Stat::Resource => "resource",
            Stat::ResourceRegen => "resource_regen",
            Stat::AttackDamage => "attack_damage",
            Stat::AbilityPower => "ability_power",
            Stat::Armor => "armor",
            Stat::MagicResist => "magic_resist",
            Stat::ArmorPen => "armor_pen",
            Stat::MagicPen => "magic_pen",
            Stat::MoveSpeed => "move_speed",
            Stat::PhysicalBlock => "physical_block",
            Stat::AttackDamagePct => "attack_damage_pct",
            Stat::AbilityPowerPct => "ability_power_pct",
            Stat::AttackSpeedPct => "attack_speed_pct",
            Stat::MoveSpeedPct => "move_speed_pct",
            Stat::HealingReceivedPct => "healing_received_pct",
            Stat::DamageDealtPct => "damage_dealt_pct",
            Stat::AttackSpeed => "attack_speed",
            Stat::CritChance => "crit_chance",
            Stat::ArmorPenPct => "armor_pen_pct",
            Stat::MagicPenPct => "magic_pen_pct",
            Stat::LifeSteal => "life_steal",
            Stat::SpellVamp => "spell_vamp",
            Stat::CooldownReduction => "cooldown_reduction",
            Stat::Slow => "slow",
        }
    }
}

impl<'de> Deserialize<'de> for Stat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Stat, D::Error> {
        let name = String::deserialize(deserializer)?;
        Stat::named(&name).ok_or_else(|| D::Error::custom(format!("unknown stat {name:?}")))
    }
}
