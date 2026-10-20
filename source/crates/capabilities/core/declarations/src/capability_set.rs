use bevy_ecs::resource::Resource;
use campfire_sim::Capability;

/// A match's declared capabilities: not `mode`, which every match has. `Release::capability_set`
/// makes one of a mode's, each with the ones it builds on.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilitySet(u32);

const _: () = assert!(
    Capability::ALL.len() <= u32::BITS as usize,
    "a set holds each capability in a bit of its u32"
);

impl CapabilitySet {
    /// No capability: a match of the core alone.
    pub const CORE: Self = Self(0);

    pub const fn contains(self, capability: Capability) -> bool {
        self.0 & Self::bit(capability) != 0
    }

    /// Whether a match of the set runs the module of `capability`: the core's, `None`, and the
    /// mode's, in every match.
    pub const fn runs(self, capability: Option<Capability>) -> bool {
        match capability {
            Some(Capability::Mode) | None => true,
            Some(capability) => self.contains(capability),
        }
    }

    /// The set with `capability`, which is not `mode`.
    #[must_use]
    pub const fn with(self, capability: Capability) -> Self {
        assert!(
            !matches!(capability, Capability::Mode),
            "every match has the mode"
        );
        Self(self.0 | Self::bit(capability))
    }

    /// The declared capabilities, in the order of their indices.
    pub fn iter(self) -> impl Iterator<Item = Capability> {
        Capability::ALL
            .into_iter()
            .filter(move |&capability| self.contains(capability))
    }

    const fn bit(capability: Capability) -> u32 {
        1 << capability as u32
    }
}
