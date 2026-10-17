use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use serde::Deserialize;

use crate::action_data::ActionData;

/// A package's `[actions]`: its actions, by id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ActionsData(pub BTreeMap<DeclaredName, ActionData>);

impl Section for ActionsData {
    const TABLE: SectionTable = SectionTable::Package;
    const KEY: &'static str = "actions";
}
