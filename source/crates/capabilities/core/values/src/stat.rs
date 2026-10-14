use derive_more::Display;
use serde::{Deserialize, Deserializer};

use crate::declared_name::DeclaredName;

/// A stat, as data and `unit.stat(name)` name it: one a capability reads, or one the mode
/// declares for its own scripts and modifiers.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stat {
    #[display("{}", _0.name())]
    Engine(EngineStat),
    #[display("{_0}")]
    Declared(DeclaredStat),
}

/// The name of a stat the mode declares, which no engine stat has, as `Stat::declared` gives it:
/// only a stat that reads a name builds one, so no engine stat has a second form.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeclaredStat(DeclaredName);

campfire_common::enum_all! {
    /// A stat a capability reads, whatever the mode.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum EngineStat {
        MoveSpeed,
        TurnRate,
    }
    pub const ALL;
}

impl EngineStat {
    /// The stat named `name`.
    pub fn named(name: &str) -> Option<EngineStat> {
        EngineStat::ALL.into_iter().find(|stat| stat.name() == name)
    }

    /// The stat as data and scripts name it.
    pub const fn name(self) -> &'static str {
        match self {
            EngineStat::MoveSpeed => "move_speed",
            EngineStat::TurnRate => "turn_rate",
        }
    }
}

impl Stat {
    /// The stat `name` names: an engine stat by its name, else a declared one; `None` when it is
    /// not a name at all. Whether the mode declares it is the package load's check.
    pub fn named(name: &str) -> Option<Stat> {
        EngineStat::named(name)
            .map(Stat::Engine)
            .or_else(|| DeclaredName::new(name).map(|name| Stat::Declared(DeclaredStat(name))))
    }

    /// The stat as data and scripts name it.
    pub fn name(&self) -> &str {
        match self {
            Stat::Engine(stat) => stat.name(),
            Stat::Declared(declared) => declared.0.as_str(),
        }
    }

    /// The name of a stat the mode must declare; `None` for an engine stat.
    pub const fn declared(&self) -> Option<&DeclaredName> {
        match self {
            Stat::Engine(_) => None,
            Stat::Declared(declared) => Some(&declared.0),
        }
    }
}

impl<'de> Deserialize<'de> for Stat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Stat, D::Error> {
        let name = DeclaredName::deserialize(deserializer)?;
        let engine = EngineStat::named(name.as_str());
        Ok(engine.map_or(Stat::Declared(DeclaredStat(name)), Stat::Engine))
    }
}

#[cfg(test)]
mod tests {
    use serde::de::{IntoDeserializer, value};

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
        // Its name is what names it, an engine stat's or a declared one's.
        for name in ["armor", "move_speed"] {
            assert_eq!(Stat::named(name).unwrap().name(), name);
        }
        // An engine stat's name reads as the engine stat, so it has no declared form.
        let read = |name: &str| {
            Stat::deserialize(IntoDeserializer::<value::Error>::into_deserializer(name)).unwrap()
        };
        assert_eq!(read("move_speed"), Stat::Engine(EngineStat::MoveSpeed));
        assert_eq!(read("turn_rate"), Stat::Engine(EngineStat::TurnRate));
        assert_eq!(read("armor"), armor);
    }
}
