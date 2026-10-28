use campfire_common::NamedEnum;
use derive_more::Display;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use campfire_common::DeclaredName;

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

/// A stat a capability reads, whatever the mode, by the name data and scripts give it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, NamedEnum)]
pub enum EngineStat {
    MoveSpeed,
    TurnRate,
    /// How fast a walker of a movement model that keeps a velocity gathers speed, in meters a
    /// second squared (driving.md, A type moves by its movement model).
    Acceleration,
}

impl Stat {
    /// The stat `name` names: an engine stat by its name, else a declared one; `None` when it is
    /// not a name at all. Whether the mode declares it is the package load's check.
    pub fn named(name: &str) -> Option<Self> {
        EngineStat::named(name)
            .map(Stat::Engine)
            .or_else(|| DeclaredName::new(name).map(|name| Self::Declared(DeclaredStat(name))))
    }

    /// The stat as data and scripts name it.
    pub fn name(&self) -> &str {
        match self {
            Self::Engine(stat) => stat.name(),
            Self::Declared(declared) => declared.0.as_str(),
        }
    }

    /// The name of a stat the mode must declare; `None` for an engine stat.
    pub const fn declared(&self) -> Option<&DeclaredName> {
        match self {
            Self::Engine(_) => None,
            Self::Declared(declared) => Some(&declared.0),
        }
    }
}

/// As data names it, the name `Deserialize` reads.
impl Serialize for Stat {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Stat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = DeclaredName::deserialize(deserializer)?;
        let engine = EngineStat::named(name.as_str());
        Ok(engine.map_or(Self::Declared(DeclaredStat(name)), Stat::Engine))
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
