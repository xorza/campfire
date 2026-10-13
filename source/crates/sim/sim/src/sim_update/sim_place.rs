use bevy_ecs::schedule::{InternedSystemSet, SystemSet};

use crate::sim_update::{SimEdge, SimSet};

/// Where a module's system set runs: in a stage of the tick, or in a gap around one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimPlace {
    Stage(SimSet),
    Edge(SimEdge),
}

impl SimPlace {
    /// Every place, in the order a tick runs them: `SimEdge::Start`, then each stage with the
    /// gap after it.
    pub const ALL: [SimPlace; 1 + 2 * SimSet::ALL.len()] = {
        let mut all = [SimPlace::Edge(SimEdge::Start); 1 + 2 * SimSet::ALL.len()];
        let mut at = 0;
        while at < SimSet::ALL.len() {
            let stage = SimSet::ALL[at];
            all[1 + 2 * at] = SimPlace::Stage(stage);
            all[2 + 2 * at] = SimPlace::Edge(SimEdge::After(stage));
            at += 1;
        }
        all
    };

    /// The set of the sim schedule it names.
    pub fn set(self) -> InternedSystemSet {
        match self {
            SimPlace::Stage(stage) => stage.intern(),
            SimPlace::Edge(edge) => edge.intern(),
        }
    }
}
