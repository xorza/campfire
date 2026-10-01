use serde::{Deserialize, Serialize};

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
    const ALL: [Attitude; 3] = [Attitude::Hostile, Attitude::Neutral, Attitude::Friendly];

    /// The attitude `name` names.
    pub fn named(name: &str) -> Option<Attitude> {
        Attitude::ALL
            .into_iter()
            .find(|attitude| attitude.name() == name)
    }

    /// The attitude as data and scripts name it.
    pub const fn name(self) -> &'static str {
        match self {
            Attitude::Hostile => "hostile",
            Attitude::Neutral => "neutral",
            Attitude::Friendly => "friendly",
        }
    }

    /// Whether a unit may attack a unit of a team it regards so: a hostile or a neutral one.
    pub const fn may_attack(self) -> bool {
        matches!(self, Attitude::Hostile | Attitude::Neutral)
    }
}
