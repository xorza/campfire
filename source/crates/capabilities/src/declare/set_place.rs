use bevy_ecs::schedule::{InternedSystemSet, SystemSet};
use campfire_sim::SimEdge;

use crate::declare::phase::Phase;

/// Where a module set runs: a phase of a stage, or a gap between stages, whose sets run in the
/// order the schedule picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetPlace {
    Phase(Phase),
    Gap(SimEdge),
}

impl SetPlace {
    /// The set of the sim schedule it names.
    pub(crate) fn set(self) -> InternedSystemSet {
        match self {
            SetPlace::Phase(phase) => phase.intern(),
            SetPlace::Gap(edge) => edge.intern(),
        }
    }
}
