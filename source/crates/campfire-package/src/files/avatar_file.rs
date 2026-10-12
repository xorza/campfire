use campfire_capabilities::{PackageContent, UnitTypeData};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::files::content_table::ContentTable;
use crate::message_id::MessageId;

/// An avatar package's `data/avatar.toml`: the name players see, its one unit type, whose
/// fields sit at the top of the table, and its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarFile {
    /// The message of the name players see.
    pub name: MessageId,
    pub unit: UnitTypeData,
    pub content: PackageContent,
}

impl<'de> Deserialize<'de> for AvatarFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<AvatarFile, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            name: MessageId,
            #[serde(flatten)]
            rest: toml::Table,
        }
        let Fields { name, mut rest } = Fields::deserialize(deserializer)?;
        let content = ContentTable::take(&mut rest)?;
        let unit = UnitTypeData::deserialize(toml::Value::Table(rest)).map_err(D::Error::custom)?;
        Ok(AvatarFile {
            name,
            unit,
            content,
        })
    }
}
