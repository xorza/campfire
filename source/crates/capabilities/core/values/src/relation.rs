use campfire_common::NamedEnum;
use serde::{Deserialize, Serialize};

use crate::engine_enum::EngineEnum;
use crate::script_enum::ScriptEnum;

campfire_common::enum_all! {
    /// How one team regards another, as the mode's `[[relations]]` declare it and
    /// `ctx.set_relation` changes it: Unreal's team attitudes. A neutral unit may be attacked,
    /// but does not seek a fight.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Relation {
        Hostile,
        Neutral,
        Friendly,
    }
    const ALL;
    const IDENTS;
}

impl Relation {
    /// Whether a unit may attack a unit of a team it regards so: a hostile or a neutral one.
    pub const fn may_attack(self) -> bool {
        matches!(self, Self::Hostile | Self::Neutral)
    }
}

/// As data names each.
impl NamedEnum for Relation {
    const ALL: &'static [Self] = &Self::ALL;
    const NAMES: &'static [&'static str] = &["hostile", "neutral", "friendly"];
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
