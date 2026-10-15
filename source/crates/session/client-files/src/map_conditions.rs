use campfire_values::DeclaredName;
use serde::{Deserialize, Serialize};

/// A map's `client/maps/<name>/conditions.toml`: the conditions it draws its looks in, as a night
/// or snowy weather, by the names its game gives them; a look draws its variant that best matches
/// them. A map of no file has none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapConditions {
    pub conditions: Vec<DeclaredName>,
}

impl MapConditions {
    /// The file of a map, by its name.
    pub fn path(map: &str) -> String {
        format!("client/maps/{map}/conditions.toml")
    }
}
