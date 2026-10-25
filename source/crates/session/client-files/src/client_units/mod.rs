use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;
use crate::client_unit::{ClientUnit, LookDraw};
use crate::condition_test::ConditionTest;
use crate::object_look::ObjectLook;

/// A package's `client/units.toml`: how each unit type looks, `[units.<type>]`; what each
/// condition its looks' states name tests of a unit, `[conditions.<name>]`; and each look its
/// maps' client objects name, `[objects.<look>]`, which a game may draw otherwise than its units
/// (09-zero-hour.md, The multiplayer rule for client objects).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ClientUnits {
    pub units: BTreeMap<DeclaredName, ClientUnit>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub conditions: BTreeMap<DeclaredName, ConditionTest>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub objects: BTreeMap<DeclaredName, ObjectLook>,
}

impl ClientUnits {
    /// Where a package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/units.toml")
    }

    /// The first condition a draw of its looks names, of a state or of its ignore, that its
    /// conditions give no test, so no client could tell whether a unit holds it.
    pub fn untested(&self) -> Option<&DeclaredName> {
        let mut names = self
            .units
            .values()
            .flat_map(ClientUnit::every_draw)
            .flat_map(LookDraw::conditions);
        names.find(|name| !self.conditions.contains_key(*name))
    }
}

impl ClientFile for ClientUnits {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

/// A package's file is untrusted, so a state that names a condition of no test fails to read.
impl<'de> Deserialize<'de> for ClientUnits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            units: BTreeMap<DeclaredName, ClientUnit>,
            #[serde(default)]
            conditions: BTreeMap<DeclaredName, ConditionTest>,
            #[serde(default)]
            objects: BTreeMap<DeclaredName, ObjectLook>,
        }
        let Fields {
            units,
            conditions,
            objects,
        } = Fields::deserialize(deserializer)?;
        let units = Self {
            units,
            conditions,
            objects,
        };
        if let Some(name) = units.untested() {
            return Err(D::Error::custom(format!(
                "a look's state names the condition `{name}`, of no test"
            )));
        }
        Ok(units)
    }
}

#[cfg(test)]
mod tests;
