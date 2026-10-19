use campfire_common::{DeclaredName, MapName};
use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::client_path;

/// A map's `client/maps/<name>/conditions.toml`: the conditions it draws its looks in, as a night
/// or snowy weather, by the names its game gives them; a look draws its variant that best matches
/// them. A map of no file has none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapConditions {
    pub conditions: Vec<DeclaredName>,
}

impl MapConditions {
    /// The file of the map `map`.
    pub fn path(map: &MapName) -> PackagePath {
        client_path::of_map(map, "conditions.toml")
    }
}
