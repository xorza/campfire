use campfire_common::SegmentSeed;
use campfire_declare::{CapabilitySet, Role};
use campfire_sim::TickRate;
use campfire_visibility::OutView;

/// What a match is built of: its tick rate and seed, its capabilities, its role, and what its
/// players out of play see.
#[derive(Debug, Clone)]
#[must_use = "a spec builds nothing until a match is built of it"]
pub struct MatchSpec {
    pub(crate) seed: SegmentSeed,
    pub(crate) rate: TickRate,
    pub(crate) set: CapabilitySet,
    pub(crate) role: Role,
    pub(crate) out: OutView,
    /// Whether the tick's schedule leaves out Bevy's automatic sync points, for the ambiguity
    /// check that sees every pair only a sync point keeps in order.
    #[cfg(feature = "internals")]
    pub(crate) without_sync_points: bool,
    /// Whether the build leaves the tick's schedule unbuilt, its systems in its graph, for a test
    /// that reads what each one accesses.
    #[cfg(feature = "internals")]
    pub(crate) unbuilt: bool,
}

impl MatchSpec {
    /// A match at `rate`, drawing from `seed`, of `role`, of no capability but the core until the
    /// spec says otherwise. A client's predicts: each of its sim's queries skips the units the
    /// client holds as the server sent them, `Unpredicted`, unless it names them, as the marker's
    /// filter is in place before any system's query is made.
    pub const fn new(seed: SegmentSeed, rate: TickRate, role: Role) -> Self {
        Self {
            seed,
            rate,
            set: CapabilitySet::CORE,
            role,
            out: OutView::Team,
            #[cfg(feature = "internals")]
            without_sync_points: false,
            #[cfg(feature = "internals")]
            unbuilt: false,
        }
    }

    /// The match's capabilities.
    pub fn set(self, set: CapabilitySet) -> Self {
        Self { set, ..self }
    }

    /// The match of a session whose players out of play see as `out` says, as its terms carry it;
    /// `OutView::Team` until the spec says otherwise.
    pub fn out(self, out: OutView) -> Self {
        Self { out, ..self }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::release::match_spec::MatchSpec;

    impl MatchSpec {
        /// The match with no automatic sync points in its tick's schedule, so its build panics
        /// on every pair of systems that only a sync point keeps in order.
        pub fn without_sync_points(self) -> Self {
            Self {
                without_sync_points: true,
                ..self
            }
        }

        /// The match with its tick's schedule left unbuilt in its world, each system in the
        /// schedule's graph, so a test reads what each accesses; it runs no tick.
        pub fn unbuilt(self) -> Self {
            Self {
                unbuilt: true,
                ..self
            }
        }
    }
}
