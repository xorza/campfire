use campfire_common::NamedEnum;
use campfire_derive::{EnumAll, EnumIdents, NamedEnum};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::engine_enum::EngineEnum;
use crate::script_enum::ScriptEnum;

/// How one team regards another, as the mode's `[[relations]]` declare it and
/// `ctx.set_relation` changes it: Unreal's team attitudes. A neutral unit may be attacked,
/// but does not seek a fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll, EnumIdents, NamedEnum)]
pub enum Relation {
    Hostile,
    Neutral,
    Friendly,
}

impl Relation {
    /// Whether a unit may attack a unit of a team it regards so: a hostile or a neutral one.
    pub const fn may_attack(self) -> bool {
        matches!(self, Self::Hostile | Self::Neutral)
    }
}

/// Written and read as serde's derive does a unit variant: a binary format, as a snapshot, by its
/// place, and a text format, as data, by its name.
impl Serialize for Relation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.serialize_member(serializer)
    }
}

impl<'de> Deserialize<'de> for Relation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize_member(deserializer)
    }
}

/// `Relation::Hostile` and the others in scripts.
impl ScriptEnum for Relation {
    const ENUM: EngineEnum = EngineEnum::Relation;
    const SCRIPT_NAMES: &'static [&'static str] = &Self::IDENTS;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script_enum::internals::named_as_data;

    #[test]
    fn each_relation_is_named_in_scripts_as_data_names_it() {
        named_as_data::<Relation>();
    }
}
