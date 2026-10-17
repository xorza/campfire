use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use serde::Deserialize;

use crate::modifier_data::ModifierData;

/// A package's `[modifiers]`: its modifiers, by id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ModifiersData(pub BTreeMap<DeclaredName, ModifierData>);

impl Section for ModifiersData {
    const TABLE: SectionTable = SectionTable::Package;
    const KEY: &'static str = "modifiers";
}
