use campfire_common::NamedEnum;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The engine's capabilities: what a mode's manifest declares, and the owner a command names.
/// A command carries its capability as the variant's index, on the wire and in the session log,
/// so the order never changes: a new capability is added at the end. `Mode` owns the mode's own
/// inputs: every match has it, so no manifest declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, NamedEnum)]
pub enum Capability {
    Combat,
    Stats,
    Abilities,
    Projectiles,
    Areas,
    Orders,
    Navigation,
    Vision,
    Mode,
    Progression,
    Production,
    Items,
    Interaction,
    Airfields,
}

/// Written and read as serde's derive does a unit variant: a binary format, as the wire and the
/// session log, by its place, and a text format, as a manifest, by its name.
impl Serialize for Capability {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.serialize_member(serializer)
    }
}

impl<'de> Deserialize<'de> for Capability {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize_member(deserializer)
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
