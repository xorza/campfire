use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;
use crate::client_unit::ClientUnit;
use crate::object_look::ObjectLook;

/// A package's `client/units.toml`: how each unit type looks, `[units.<type>]`, and each look
/// its maps' client objects name, `[objects.<look>]`, which a game may draw otherwise than its
/// units (09-zero-hour.md, The multiplayer rule for client objects).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientUnits {
    pub units: BTreeMap<DeclaredName, ClientUnit>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub objects: BTreeMap<DeclaredName, ObjectLook>,
}

impl ClientUnits {
    /// Where a package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/units.toml")
    }
}

impl ClientFile for ClientUnits {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

#[cfg(test)]
mod tests;
