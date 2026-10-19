use std::collections::BTreeMap;

use campfire_codec::{TomlError, TomlTable, TomlValue};
use campfire_common::DeclaredName;
use campfire_declare::{ReadValue, Section, SectionReaders, SectionTable};

use crate::action_data::ActionData;

/// A package's `[actions]`: its actions, by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionsData(pub BTreeMap<DeclaredName, ActionData>);

impl Section for ActionsData {
    const TABLE: SectionTable = SectionTable::Package;
    const KEY: &'static str = "actions";
}

/// Each action read with the release's readers, which read its lists' effects.
impl ReadValue for ActionsData {
    fn read_value(value: TomlValue, readers: &SectionReaders) -> Result<Self, TomlError> {
        let tables = value.read::<BTreeMap<DeclaredName, TomlTable>>()?;
        let actions = tables
            .into_iter()
            .map(|(id, table)| Ok((id, ActionData::read(table, readers)?)));
        Ok(Self(actions.collect::<Result<_, TomlError>>()?))
    }

    /// None: its keys are the ids of the actions.
    fn fields() -> &'static [&'static str] {
        &[]
    }
}
