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
    Hitboxes,
    Navigation,
    Vision,
    Physics,
    World,
    Mode,
    Progression,
    Production,
    Items,
    Quests,
    Interaction,
}

impl Capability {
    /// Every capability, in the order of their indices.
    pub const ALL: [Capability; 18] = [
        Capability::Combat,
        Capability::Stats,
        Capability::Abilities,
        Capability::Projectiles,
        Capability::Areas,
        Capability::Orders,
        Capability::Character,
        Capability::Hitboxes,
        Capability::Navigation,
        Capability::Vision,
        Capability::Physics,
        Capability::World,
        Capability::Mode,
        Capability::Progression,
        Capability::Production,
        Capability::Items,
        Capability::Quests,
        Capability::Interaction,
    ];

    /// The capability as a manifest names it.
    pub const fn name(self) -> &'static str {
        match self {
            Capability::Combat => "combat",
            Capability::Stats => "stats",
            Capability::Abilities => "abilities",
            Capability::Projectiles => "projectiles",
            Capability::Areas => "areas",
            Capability::Orders => "orders",
            Capability::Character => "character",
            Capability::Hitboxes => "hitboxes",
            Capability::Navigation => "navigation",
            Capability::Vision => "vision",
            Capability::Physics => "physics",
            Capability::World => "world",
            Capability::Mode => "mode",
            Capability::Progression => "progression",
            Capability::Production => "production",
            Capability::Items => "items",
            Capability::Quests => "quests",
            Capability::Interaction => "interaction",
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::de::value::{Error as ValueError, StrDeserializer};

    use super::*;

    #[test]
    fn all_lists_each_capability_at_its_index_and_names_it_as_a_manifest_does() {
        for (index, capability) in Capability::ALL.into_iter().enumerate() {
            assert_eq!(capability as usize, index, "{capability:?}");
            let name = StrDeserializer::<ValueError>::new(capability.name());
            let named = Capability::deserialize(name);
            assert_eq!(named, Ok(capability));
        }
    }
}
