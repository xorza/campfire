use std::num::NonZeroU8;

use campfire_common::DeclaredName;
use campfire_script::{DataSection, DataTable};
use campfire_values::TextId;
use serde::{Deserialize, Serialize};

/// A choice of the mode's `[choices]`: the message of the name players see, what it offers, how
/// many values a player chooses, whether no two players may choose one value, for loadout
/// entries, the slot kind they fill, whose ranks they have, and for values, the names it offers,
/// in order (04-capabilities/control.md, Choices before the spawn).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceData {
    pub name: TextId,
    pub offers: Offers,
    #[serde(default)]
    pub unique: bool,
    #[serde(default = "ChoiceData::one")]
    pub count: NonZeroU8,
    pub slot: Option<DeclaredName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<DeclaredName>,
}

/// What a choice offers: the avatar packages the mode depends on, the entries of its loadout
/// packages, or the values its own data lists, which a session's host may pick for each seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Offers {
    Avatars,
    Loadout,
    Values,
}

impl ChoiceData {
    const fn one() -> NonZeroU8 {
        NonZeroU8::MIN
    }
}

impl DataSection for ChoiceData {
    const TABLE: DataTable = DataTable::new("A choice, `[choices.<name>]`");
}
