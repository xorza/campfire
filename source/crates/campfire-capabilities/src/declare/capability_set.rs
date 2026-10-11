use campfire_sim::Capability;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::declare::error::CapabilityError;

/// A mode's declared capabilities: none twice, not `mode`, which every match has, and each with
/// the ones it builds on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilitySet(u32);

const _: () = assert!(
    Capability::ALL.len() <= u32::BITS as usize,
    "a set holds each capability in a bit of its u32"
);

impl CapabilitySet {
    /// The set of `declared`; an error when one is `mode`, one is declared twice, or one lacks a
    /// capability it builds on.
    pub fn new(declared: &[Capability]) -> Result<CapabilitySet, CapabilityError> {
        let mut set = CapabilitySet(0);
        for &capability in declared {
            if capability == Capability::Mode {
                return Err(CapabilityError::DeclaresMode);
            }
            if set.contains(capability) {
                return Err(CapabilityError::Repeated(capability));
            }
            set.0 |= CapabilitySet::bit(capability);
        }
        for capability in set.iter() {
            if let Some(&needs) = CapabilitySet::needs(capability)
                .iter()
                .find(|&&needs| !set.contains(needs))
            {
                return Err(CapabilityError::Needs { capability, needs });
            }
        }
        Ok(set)
    }

    pub const fn contains(self, capability: Capability) -> bool {
        self.0 & CapabilitySet::bit(capability) != 0
    }

    /// The declared capabilities, in the order of their indices.
    pub fn iter(self) -> impl Iterator<Item = Capability> {
        Capability::ALL
            .into_iter()
            .filter(move |&capability| self.contains(capability))
    }

    /// The capabilities `capability` builds on.
    pub(crate) const fn needs(capability: Capability) -> &'static [Capability] {
        match capability {
            Capability::Progression | Capability::Combat | Capability::Items => {
                &[Capability::Stats]
            }
            Capability::Vision
            | Capability::Projectiles
            | Capability::Areas
            | Capability::Abilities => &[Capability::Combat],
            Capability::Orders => &[Capability::Combat, Capability::Navigation],
            Capability::Stats
            | Capability::Navigation
            | Capability::Production
            | Capability::Character
            | Capability::Hitboxes
            | Capability::Physics
            | Capability::World
            | Capability::Quests
            | Capability::Interaction
            | Capability::Mode => &[],
        }
    }

    const fn bit(capability: Capability) -> u32 {
        1 << capability as u32
    }
}

/// A manifest's `capabilities`, checked as `CapabilitySet::new` checks them.
impl<'de> Deserialize<'de> for CapabilitySet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CapabilitySet, D::Error> {
        let declared = Vec::<Capability>::deserialize(deserializer)?;
        CapabilitySet::new(&declared).map_err(D::Error::custom)
    }
}
