use campfire_capabilities::{ModeData, PackageContent};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::files::content_table::ContentTable;
use crate::files::mode_script::ModeScript;

/// The mode's `data/mode.toml`: its script, the mode's own data, and its actions and modifiers,
/// which its content holds beside the unit types of `data/units.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModeFile {
    pub(crate) script: ModeScript,
    pub(crate) data: ModeData,
    pub(crate) content: PackageContent,
}

impl<'de> Deserialize<'de> for ModeFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ModeFile, D::Error> {
        let mut table = toml::Table::deserialize(deserializer)?;
        if table.contains_key("units") {
            return Err(D::Error::custom("units are in data/units.toml"));
        }
        let script = table
            .remove("script")
            .ok_or_else(|| D::Error::missing_field("script"))?;
        let script = ModeScript::deserialize(script).map_err(D::Error::custom)?;
        let content = ContentTable::take(&mut table)?;
        let data = ModeData::deserialize(toml::Value::Table(table)).map_err(D::Error::custom)?;
        Ok(ModeFile {
            script,
            data,
            content,
        })
    }
}
