use std::collections::BTreeMap;

use campfire_capabilities::{ActionData, ModifierData};
use serde::Deserialize;

use crate::files::units_data::UnitTypeFile;

/// A loadout package's `data/loadout.toml`: the actions players choose beside their avatar's,
/// each with the ranks of the slot kind its choice fills, the modifiers they apply, and the unit
/// types they deliver, by id.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadoutData {
    #[serde(default)]
    pub actions: BTreeMap<String, ActionData>,
    #[serde(default)]
    pub modifiers: BTreeMap<String, ModifierData>,
    #[serde(default)]
    pub units: BTreeMap<String, UnitTypeFile>,
}
