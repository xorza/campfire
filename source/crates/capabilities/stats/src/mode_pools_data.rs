use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use serde::Deserialize;

use crate::pool_data::PoolData;

/// The mode's `[pools]`, by name, the life pool among them: the amounts its units spend and lose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ModePoolsData(pub BTreeMap<DeclaredName, PoolData>);

impl Section for ModePoolsData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "pools";
}
