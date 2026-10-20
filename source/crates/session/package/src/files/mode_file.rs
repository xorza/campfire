use campfire_capabilities::{PackageContent, Release};
use campfire_codec::TomlTable;
use campfire_declarations::UnitDefaults;
use campfire_mode::ModeData;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::files::content_table::ContentTable;
use crate::files::script_list::ScriptList;

/// The mode's `data/mode.toml`: its scripts, the defaults of its unit types' sections, the mode's
/// own data, and its actions and modifiers, which its content holds beside the unit types of
/// `data/units.toml`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ModeFile {
    pub(crate) scripts: ScriptList,
    pub(crate) defaults: UnitDefaults,
    pub(crate) data: ModeData,
    pub(crate) content: PackageContent,
}

impl ModeFile {
    /// The key of its list of scripts, the mode's alone: no rules package's data gives it.
    pub(crate) const SCRIPTS: &'static str = "scripts";

    /// The key of the defaults of its unit types' sections.
    const DEFAULTS: &'static str = "defaults";
}

impl<'de> Deserialize<'de> for ModeFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut table = TomlTable::deserialize(deserializer)?;
        if table.holds("units") {
            return Err(D::Error::custom("units are in data/units.toml"));
        }
        let scripts: ScriptList = table
            .take(Self::SCRIPTS)
            .ok_or_else(|| D::Error::missing_field(Self::SCRIPTS))?
            .map_err(D::Error::custom)?;
        let readers = Release::sections();
        let defaults = match table.take_value(Self::DEFAULTS) {
            Some(value) => UnitDefaults::read(value, &readers).map_err(D::Error::custom)?,
            None => UnitDefaults::default(),
        };
        let content = ContentTable::take(&mut table, &defaults)?;
        let data = table.read_with(ModeData::seed(&readers));
        let data = data.map_err(D::Error::custom)?;
        Ok(Self {
            scripts,
            defaults,
            data,
            content,
        })
    }
}
