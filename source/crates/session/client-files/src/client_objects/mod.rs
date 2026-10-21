use campfire_common::{DeclaredName, MapName};
use campfire_math::Num;
use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;

/// A map's `client/maps/<name>/objects.toml`: the objects the client alone draws, still, of which
/// the sim holds none (09-zero-hour.md, The multiplayer rule for client objects). Each names its
/// look among the `objects` of its package's `client/units.toml`, and stands as a placed unit does:
/// at a point of the ground plane, a height above the ground there, and an angle in degrees, each
/// an exact decimal.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientObjects {
    pub objects: Vec<ClientObject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientObject {
    pub look: DeclaredName,
    pub position: [Num; 2],
    pub height: Num,
    pub angle: Num,
}

impl ClientObjects {
    /// The file of the map `map`.
    pub fn path(map: &MapName) -> PackagePath {
        client_path::of_map(map, "objects.toml")
    }
}

impl ClientFile for ClientObjects {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

#[cfg(test)]
mod tests;
