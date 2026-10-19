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
    /// How the build ends, for a test.
    #[cfg(feature = "internals")]
    pub(crate) end: internals::BuildEnd,
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
            end: internals::BuildEnd::Built,
        }
    }

    /// The rate the match ticks at.
    pub const fn rate(&self) -> TickRate {
        self.rate
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

    /// How a test's match build ends.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum BuildEnd {
        /// With the tick's schedule built, as a match's.
        Built,
        /// With the tick's schedule unbuilt, each system in its graph, for a test that reads what
        /// each accesses: the match runs no tick.
        Unbuilt,
        /// With the systems not installed, for a test that loads its books and types after the
        /// build: its first tick installs them, as their books then are, and builds the schedule.
        Open,
    }

    impl BuildEnd {
        /// Whether the build leaves the tick's schedule unbuilt.
        pub(crate) const fn leaves_unbuilt(self) -> bool {
            matches!(self, Self::Unbuilt)
        }

        /// Whether the build leaves the match's systems for its first tick to install.
        pub(crate) const fn leaves_open(self) -> bool {
            matches!(self, Self::Open)
        }
    }

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
                end: BuildEnd::Unbuilt,
                ..self
            }
        }

        /// The match with its systems not installed until its first tick, so a test loads its
        /// books and types first, and the install reads them as a match's reads its packages'.
        pub(crate) fn open(self) -> Self {
            Self {
                end: BuildEnd::Open,
                ..self
            }
        }
    }
}
