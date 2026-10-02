use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::files::package_content::PackageContent;
use crate::files::units_data::UnitTypeFile;

/// An avatar package's `data/avatar.toml`: the name players see, its one unit type, whose
/// fields sit at the top of the table, and its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarData {
    pub name: String,
    pub unit: UnitTypeFile,
    pub content: PackageContent,
}

impl<'de> Deserialize<'de> for AvatarData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<AvatarData, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            name: String,
            #[serde(flatten)]
            rest: toml::Table,
        }
        let Fields { name, mut rest } = Fields::deserialize(deserializer)?;
        let content = PackageContent::take(&mut rest)?;
        let unit = UnitTypeFile::deserialize(toml::Value::Table(rest)).map_err(D::Error::custom)?;
        Ok(AvatarData {
            name,
            unit,
            content,
        })
    }
}
