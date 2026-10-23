use campfire_capabilities::{PackageContent, Release};
use campfire_codec::TomlTable;
use campfire_declarations::UnitDefaults;
use campfire_mode::ModeData;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::files::bot_list::BotList;
use crate::files::content_table::ContentTable;
use crate::files::script_list::ScriptOrder;

/// The mode's `data/mode.toml`: its scripts, its bots, the defaults of its unit types' sections,
/// the mode's own data, and its actions and modifiers, which its content holds beside the unit
/// types of `data/units.toml`.
#[derive(Debug, Clone, PartialEq)]
pub struct ModeFile {
    pub(crate) scripts: ScriptOrder,
    pub(crate) bots: BotList,
    pub(crate) defaults: UnitDefaults,
    pub(crate) data: ModeData,
    pub(crate) content: PackageContent,
}

impl ModeFile {
    /// Where a mode's package holds it.
    pub fn path() -> PackagePath {
        PackagePath::parse("data/mode.toml").expect("a package path")
    }

    /// The key of the defaults of its unit types' sections.
    const DEFAULTS: &'static str = "defaults";
}

impl<'de> Deserialize<'de> for ModeFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut table = TomlTable::deserialize(deserializer)?;
        if table.holds("units") {
            return Err(D::Error::custom("units are in data/units.toml"));
        }
        let scripts: ScriptOrder = table
            .take(ScriptOrder::KEY)
            .ok_or_else(|| D::Error::missing_field(ScriptOrder::KEY))?
            .map_err(D::Error::custom)?;
        let bots = match table.take(BotList::KEY) {
            Some(bots) => bots.map_err(D::Error::custom)?,
            None => BotList::default(),
        };
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
            bots,
            defaults,
            data,
            content,
        })
    }
}
