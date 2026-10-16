use campfire_capabilities::{PackageContent, UnitTypeData};
use campfire_codec::TomlTable;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::files::content_table::ContentTable;
use campfire_values::MessageId;

/// An avatar package's `data/avatar.toml`: the name players see, its one unit type, whose
/// fields sit at the top of the table, and its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AvatarFile {
    /// The message of the name players see.
    pub(crate) name: MessageId,
    pub(crate) unit: UnitTypeData,
    pub(crate) content: PackageContent,
}

impl<'de> Deserialize<'de> for AvatarFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            name: MessageId,
            #[serde(flatten)]
            rest: TomlTable,
        }
        let Fields { name, mut rest } = Fields::deserialize(deserializer)?;
        let content = ContentTable::take(&mut rest)?;
        let unit: UnitTypeData = rest.read().map_err(D::Error::custom)?;
        Ok(Self {
            name,
            unit,
            content,
        })
    }
}
