use campfire_common::SegmentSeed;
use campfire_sim::TickRate;

use crate::declare::capability_set::CapabilitySet;
use crate::scripts::script_budgets::ScriptBudgets;

/// What a match is built of: its tick rate and seed, its capabilities, and the budgets of its
/// scripts.
#[derive(Debug, Clone)]
#[must_use = "a spec builds nothing until a match is built of it"]
pub struct MatchSpec {
    pub(crate) seed: SegmentSeed,
    pub(crate) rate: TickRate,
    pub(crate) set: CapabilitySet,
    /// `None` for a match that runs no scripts.
    pub(crate) budgets: Option<ScriptBudgets>,
    /// Whether a client predicts the match: its sim's queries skip the units it holds as the
    /// server sent them.
    pub(crate) predicting: bool,
    /// Whether the tick's schedule leaves out Bevy's automatic sync points, for the ambiguity
    /// check that sees every pair only a sync point keeps in order.
    #[cfg(any(test, feature = "internals"))]
    pub(crate) without_sync_points: bool,
    /// Whether the build leaves the tick's schedule unbuilt, its systems in its graph, for a test
    /// that reads what each one accesses.
    #[cfg(any(test, feature = "internals"))]
    pub(crate) unbuilt: bool,
}

impl MatchSpec {
    /// A match at `rate`, drawing from `seed`: of no capability but the core, and running no
    /// scripts, until the spec says otherwise.
    pub const fn new(seed: SegmentSeed, rate: TickRate) -> MatchSpec {
        MatchSpec {
            seed,
            rate,
            set: CapabilitySet::CORE,
            budgets: None,
            predicting: false,
            #[cfg(any(test, feature = "internals"))]
            without_sync_points: false,
            #[cfg(any(test, feature = "internals"))]
            unbuilt: false,
        }
    }

    /// The match's capabilities.
    pub fn capabilities(self, set: CapabilitySet) -> MatchSpec {
        MatchSpec { set, ..self }
    }

    /// The match runs scripts within `budgets`, as a server or a verifier does. A match without
    /// runs none, as a client, which predicts only its own player's units: the core has no
    /// script host, and each capability leaves out what runs scripts.
    pub fn script_budgets(self, budgets: ScriptBudgets) -> MatchSpec {
        MatchSpec {
            budgets: Some(budgets),
            ..self
        }
    }

    /// The match a client predicts: each of its sim's queries skips the units the client holds
    /// as the server sent them, `Unpredicted`, unless it names them, as the marker's filter is in
    /// place before any system's query is made.
    pub fn predicting(self) -> MatchSpec {
        MatchSpec {
            predicting: true,
            ..self
        }
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use crate::release::match_spec::MatchSpec;

    impl MatchSpec {
        /// The match with no automatic sync points in its tick's schedule, so its build panics
        /// on every pair of systems that only a sync point keeps in order.
        pub fn without_sync_points(self) -> MatchSpec {
            MatchSpec {
                without_sync_points: true,
                ..self
            }
        }

        /// The match with its tick's schedule left unbuilt in its world, each system in the
        /// schedule's graph, so a test reads what each accesses; it runs no tick.
        pub fn unbuilt(self) -> MatchSpec {
            MatchSpec {
                unbuilt: true,
                ..self
            }
        }
    }
}
