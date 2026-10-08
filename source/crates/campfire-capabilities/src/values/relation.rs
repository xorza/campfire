use serde::{Deserialize, Serialize};

use crate::values::engine_enum::EngineEnum;
use crate::values::script_enum::ScriptEnum;

/// How one team regards another, as the mode's `[[relations]]` declare it and
/// `ctx.set_relation` changes it: Unreal's team attitudes. A neutral unit may be attacked, but
/// does not seek a fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Hostile,
    Neutral,
    Friendly,
}

impl Relation {
    /// Whether a unit may attack a unit of a team it regards so: a hostile or a neutral one.
    pub const fn may_attack(self) -> bool {
        matches!(self, Relation::Hostile | Relation::Neutral)
    }
}

/// `Relation::Hostile` and the others in scripts.
impl ScriptEnum for Relation {
    const ENUM: EngineEnum = EngineEnum::Relation;
    const MEMBERS: &'static [(&'static str, Relation)] = &[
        ("Hostile", Relation::Hostile),
        ("Neutral", Relation::Neutral),
        ("Friendly", Relation::Friendly),
    ];

    fn data_name(self) -> &'static str {
        match self {
            Relation::Hostile => "hostile",
            Relation::Neutral => "neutral",
            Relation::Friendly => "friendly",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::script_enum::internals::named_as_data;

    #[test]
    fn each_relation_is_named_in_scripts_as_data_names_it() {
        named_as_data::<Relation>();
    }
}
