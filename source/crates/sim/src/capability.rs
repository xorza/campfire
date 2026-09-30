use serde::{Deserialize, Serialize};

/// The engine's capabilities: what a mode's manifest declares, and the owner a command names.
/// A command carries its capability as the variant's index, on the wire and in the session log,
/// so the order never changes: a new capability is added at the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Combat,
    Stats,
    Abilities,
    Projectiles,
    Areas,
    Orders,
    Character,
    Hitscan,
    Navigation,
    Vision,
    Physics,
    Persistence,
}
