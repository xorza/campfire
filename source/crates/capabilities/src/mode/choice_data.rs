use std::num::NonZeroU8;

use serde::Deserialize;

use crate::values::declared_name::DeclaredName;

/// A choice of the mode's `[choices]`: what it offers, how many values a player chooses, whether
/// no two players may choose one value, and for loadout entries, the slot kind they fill, whose
/// ranks they have.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceData {
    pub offers: Offers,
    #[serde(default)]
    pub unique: bool,
    #[serde(default = "ChoiceData::one")]
    pub count: NonZeroU8,
    pub slot: Option<DeclaredName>,
}

/// What a choice offers: the avatar packages the mode depends on, or the entries of its loadout
/// packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Offers {
    Avatars,
    Loadout,
}

impl ChoiceData {
    const fn one() -> NonZeroU8 {
        NonZeroU8::MIN
    }
}
