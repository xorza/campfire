use bevy_ecs::schedule::{InternedSystemSet, SystemSet};
use campfire_sim::SimEdge;

use crate::phase::Phase;

/// Where a module set runs: a phase of a stage, or a gap between stages, whose sets run in the
/// order the schedule picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetPlace {
    Phase(Phase),
    Gap(SimEdge),
}

impl SetPlace {
    /// The set of the sim schedule it names.
    pub fn set(self) -> InternedSystemSet {
        match self {
            Self::Phase(phase) => phase.intern(),
            Self::Gap(edge) => edge.intern(),
        }
    }
}
