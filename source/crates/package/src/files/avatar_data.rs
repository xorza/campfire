use std::collections::BTreeMap;

use campfire_capabilities::{ActionData, ModifierData};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::files::units_data::UnitTypeFile;

/// An avatar package's `data/avatar.toml`: its one unit type, in the units schema, with its
/// name, and the actions and modifiers of the package. An avatar carries the tag `avatar`, and
/// stays when it dies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarData {
    pub name: String,
    pub unit: UnitTypeFile,
    pub actions: BTreeMap<String, ActionData>,
    pub modifiers: BTreeMap<String, ModifierData>,
}

/// The unit type's fields beside the package's own, in one table.
impl<'de> Deserialize<'de> for AvatarData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<AvatarData, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            name: String,
            #[serde(default)]
            actions: BTreeMap<String, ActionData>,
            #[serde(default)]
            modifiers: BTreeMap<String, ModifierData>,
            #[serde(flatten)]
            unit: toml::Table,
        }
        let fields = Fields::deserialize(deserializer)?;
        let unit =
            UnitTypeFile::deserialize(toml::Value::Table(fields.unit)).map_err(D::Error::custom)?;
        Ok(AvatarData {
            name: fields.name,
            unit,
            actions: fields.actions,
            modifiers: fields.modifiers,
        })
    }
}
