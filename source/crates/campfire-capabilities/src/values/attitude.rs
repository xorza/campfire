use serde::{Deserialize, Serialize};

use crate::values::engine_enum::EngineEnum;
use crate::values::script_enum::ScriptEnum;

/// How one team regards another, as the mode's `[[relations]]` declare it and
/// `ctx.set_relation` changes it: Unreal's team attitudes. A neutral unit may be attacked, but
/// does not seek a fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attitude {
    Hostile,
    Neutral,
    Friendly,
}

impl Attitude {
    /// Whether a unit may attack a unit of a team it regards so: a hostile or a neutral one.
    pub const fn may_attack(self) -> bool {
        matches!(self, Attitude::Hostile | Attitude::Neutral)
    }
}

/// `Relation::Hostile` and the others in scripts.
impl ScriptEnum for Attitude {
    const ENUM: EngineEnum = EngineEnum::Relation;
    const MEMBERS: &'static [(&'static str, Attitude)] = &[
        ("Hostile", Attitude::Hostile),
        ("Neutral", Attitude::Neutral),
        ("Friendly", Attitude::Friendly),
    ];

    fn data_name(self) -> &'static str {
        match self {
            Attitude::Hostile => "hostile",
            Attitude::Neutral => "neutral",
            Attitude::Friendly => "friendly",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::script_enum::internals::named_as_data;

    #[test]
    fn each_relation_is_named_in_scripts_as_data_names_it() {
        named_as_data::<Attitude>();
    }
}
