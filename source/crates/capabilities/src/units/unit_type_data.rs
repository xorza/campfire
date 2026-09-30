use std::collections::BTreeMap;

use serde::Deserialize;

use crate::units::scalar::Scalar;

/// A unit type's core fields as its data file declares them: its tags, which filters select, and
/// the params its scripts read as `unit.params`. Each capability the type uses reads its own
/// section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct UnitTypeData {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub params: BTreeMap<String, Scalar>,
}
