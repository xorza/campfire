use std::cmp::Ordering;

use derive_more::Display;
use serde::{Deserialize, Deserializer};

use crate::values::declared_name::DeclaredName;

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

/// A stat a capability reads, whatever the mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EngineStat {
    MoveSpeed,
}

impl EngineStat {
    pub const ALL: [EngineStat; 1] = [EngineStat::MoveSpeed];

    /// The stat named `name`.
    pub fn named(name: &str) -> Option<EngineStat> {
        EngineStat::ALL.into_iter().find(|stat| stat.name() == name)
    }

    /// The stat as data and scripts name it.
    pub const fn name(self) -> &'static str {
        match self {
            EngineStat::MoveSpeed => "move_speed",
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

    /// How it sorts against the stat `name` names, as `Ord` sorts stats, with no stat built: a
    /// name that is no stat sorts as a declared one.
    pub(crate) fn order_to(&self, name: &str) -> Ordering {
        match (self, EngineStat::named(name)) {
            (Stat::Engine(stat), Some(other)) => stat.cmp(&other),
            (Stat::Engine(_), None) => Ordering::Less,
            (Stat::Declared(_), Some(_)) => Ordering::Greater,
            (Stat::Declared(declared), None) => declared.0.as_str().cmp(name),
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
        // `order_to` sorts as `Ord` does, for every pair of an engine stat and declared ones.
        let mut stats: Vec<Stat> = ["armor", "move_speed", "zeal", "attack_damage"]
            .map(|name| Stat::named(name).unwrap())
            .into();
        stats.sort();
        for stat in &stats {
            for other in &stats {
                assert_eq!(stat.order_to(&other.to_string()), stat.cmp(other));
            }
        }
        assert_eq!(stats[0], Stat::Engine(EngineStat::MoveSpeed));
        // An engine stat's name reads as the engine stat, so it has no declared form.
        let read = |name: &str| Stat::deserialize(toml::Value::String(name.to_owned())).unwrap();
        assert_eq!(read("move_speed"), Stat::Engine(EngineStat::MoveSpeed));
        assert_eq!(read("armor"), armor);
    }
}
