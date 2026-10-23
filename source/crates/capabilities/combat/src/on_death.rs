use campfire_declarations::PerId;
use campfire_values::UnitType;
use serde::{Deserialize, Serialize};

/// What becomes of a unit when it dies, as its unit type says: in data, `stay` or `despawn`. A
/// fact of the type, which a unit's death reads through its type, as its armor is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnDeath {
    /// It stays, dead, for the mode to respawn, as avatars do.
    Stay,
    /// It despawns, as creeps and towers do.
    #[default]
    Despawn,
}

impl PerId<UnitType> for OnDeath {
    const SCRIPTS_ONLY: bool = false;
}
