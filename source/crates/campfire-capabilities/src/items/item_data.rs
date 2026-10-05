use std::collections::BTreeMap;
use std::num::NonZeroU32;

use serde::Deserialize;

use crate::values::declared_name::DeclaredName;

/// An item type, `[items.<id>]` of the mode package: what it costs in the mode's player
/// resources, whole amounts; the items of its package it is built from; how many share one
/// inventory slot; a consumable's uses, its action spending one as it resolves; the modifiers of
/// its package its carrier holds while it carries it; and the action of its package that is its
/// active.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemData {
    #[serde(default)]
    pub cost: BTreeMap<DeclaredName, u32>,
    #[serde(default)]
    pub components: Vec<DeclaredName>,
    #[serde(default = "ItemData::one")]
    pub stack: NonZeroU32,
    pub uses: Option<NonZeroU32>,
    #[serde(default)]
    pub modifiers: Vec<DeclaredName>,
    pub action: Option<DeclaredName>,
}

impl ItemData {
    /// A stack of one, an item's own slot.
    const fn one() -> NonZeroU32 {
        NonZeroU32::MIN
    }
}
