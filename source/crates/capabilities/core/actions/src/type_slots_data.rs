use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use serde::{Deserialize, Serialize};

/// A unit type's `slots`: its package's actions in each slot kind, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TypeSlotsData(pub BTreeMap<DeclaredName, Vec<DeclaredName>>);

impl Section for TypeSlotsData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "slots";
}
