use serde::{Deserialize, Serialize};

/// The engine's capabilities: what a mode's manifest declares, and the owner a command names.
/// A command carries its capability as the variant's index, on the wire and in the session log,
/// so the order never changes: a new capability is added at the end. `Mode` owns the mode's own
/// inputs: every match has it, so no manifest declares it.
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
    Mode,
}

impl Capability {
    /// Every capability, in the order of their indices.
    pub const ALL: [Capability; 13] = [
        Capability::Combat,
        Capability::Stats,
        Capability::Abilities,
        Capability::Projectiles,
        Capability::Areas,
        Capability::Orders,
        Capability::Character,
        Capability::Hitscan,
        Capability::Navigation,
        Capability::Vision,
        Capability::Physics,
        Capability::Persistence,
        Capability::Mode,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_lists_each_capability_at_its_index() {
        for (index, capability) in Capability::ALL.into_iter().enumerate() {
            assert_eq!(capability as usize, index, "{capability:?}");
        }
    }
}
