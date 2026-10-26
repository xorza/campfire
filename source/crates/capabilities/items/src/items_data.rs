use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use serde::Deserialize;

use crate::item_data::ItemData;

/// A package's `[items]`: its item types, by id; the load lets only the mode hold them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ItemsData(pub BTreeMap<DeclaredName, ItemData>);

impl Section for ItemsData {
    const TABLE: SectionTable = SectionTable::Package;
    const KEY: &'static str = "items";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
