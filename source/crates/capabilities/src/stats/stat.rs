use std::fmt;

use serde::{Deserialize, Deserializer};

use crate::values::declared_name::DeclaredName;

/// A stat, as data and `unit.stat(name)` name it: one a capability reads, or one the mode
/// declares for its own scripts and modifiers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stat {
    Engine(EngineStat),
    Declared(DeclaredName),
}

/// A stat a capability reads, whatever the mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EngineStat {
    MoveSpeed,
    AttackSpeed,
    AttackDamage,
}

impl EngineStat {
    pub const ALL: [EngineStat; 3] = [
        EngineStat::MoveSpeed,
        EngineStat::AttackSpeed,
        EngineStat::AttackDamage,
    ];

    /// The stat named `name`.
    pub fn named(name: &str) -> Option<EngineStat> {
        EngineStat::ALL.into_iter().find(|stat| stat.name() == name)
    }

    /// The stat as data and scripts name it.
    pub const fn name(self) -> &'static str {
        match self {
            EngineStat::MoveSpeed => "move_speed",
            EngineStat::AttackSpeed => "attack_speed",
            EngineStat::AttackDamage => "attack_damage",
        }
    }
}

impl Stat {
    /// The stat `name` names: an engine stat by its name, else a declared one; `None` when it is
    /// not a name at all. Whether the mode declares it is the package load's check.
    pub fn named(name: &str) -> Option<Stat> {
        EngineStat::named(name)
            .map(Stat::Engine)
            .or_else(|| DeclaredName::new(name).map(Stat::Declared))
    }

    /// The name of a stat the mode must declare; `None` for an engine stat.
    pub const fn declared(&self) -> Option<&DeclaredName> {
        match self {
            Stat::Engine(_) => None,
            Stat::Declared(name) => Some(name),
        }
    }
}

impl fmt::Display for Stat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stat::Engine(stat) => f.write_str(stat.name()),
            Stat::Declared(name) => write!(f, "{name}"),
        }
    }
}

impl<'de> Deserialize<'de> for Stat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Stat, D::Error> {
        let name = DeclaredName::deserialize(deserializer)?;
        Ok(EngineStat::named(name.as_str()).map_or(Stat::Declared(name), Stat::Engine))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stat_is_an_engine_stat_by_its_name_or_a_declared_one() {
        for stat in EngineStat::ALL {
            assert_eq!(Stat::named(stat.name()), Some(Stat::Engine(stat)));
        }
        let armor = Stat::named("armor").unwrap();
        assert_eq!(armor.declared().map(DeclaredName::as_str), Some("armor"));
        assert_eq!(Stat::Engine(EngineStat::MoveSpeed).declared(), None);
        assert_eq!(Stat::named("Armor"), None);
        assert_eq!(armor.to_string(), "armor");
    }
}
