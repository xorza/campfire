use bevy_ecs::schedule::{InternedSystemSet, SystemSet};

use crate::sim_update::{SimEdgeSystems, SimSystems};

/// Where a module's system set runs: in a stage of the tick, or in a gap around one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SimPlace {
    Stage(SimSystems),
    Edge(SimEdgeSystems),
}

impl SimPlace {
    /// Every place, in the order a tick runs them: `SimEdgeSystems::Start`, then each stage with
    /// the gap after it.
    pub(crate) const ALL: [Self; 1 + 2 * SimSystems::ALL.len()] = {
        let mut all = [Self::Edge(SimEdgeSystems::Start); 1 + 2 * SimSystems::ALL.len()];
        let mut at = 0;
        while at < SimSystems::ALL.len() {
            let stage = SimSystems::ALL[at];
            all[1 + 2 * at] = Self::Stage(stage);
            all[2 + 2 * at] = Self::Edge(SimEdgeSystems::After(stage));
            at += 1;
        }
        all
    };

    /// The set of the sim schedule it names.
    pub(crate) fn set(self) -> InternedSystemSet {
        match self {
            Self::Stage(stage) => stage.intern(),
            Self::Edge(edge) => edge.intern(),
        }
    }
}
