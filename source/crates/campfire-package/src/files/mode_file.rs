use campfire_capabilities::{ModeData, PackageContent};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::files::content_table::ContentTable;

/// The mode's `data/mode.toml`: the mode's own data, and its actions and modifiers, which its
/// content holds beside the unit types of `data/units.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModeFile {
    pub(crate) data: ModeData,
    pub(crate) content: PackageContent,
}

impl<'de> Deserialize<'de> for ModeFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ModeFile, D::Error> {
        let mut table = toml::Table::deserialize(deserializer)?;
        if table.contains_key("units") {
            return Err(D::Error::custom("units are in data/units.toml"));
        }
        let content = ContentTable::take(&mut table)?;
        let data = ModeData::deserialize(toml::Value::Table(table)).map_err(D::Error::custom)?;
        Ok(ModeFile { data, content })
    }
}
