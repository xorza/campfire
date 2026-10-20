use bevy_ecs::resource::Resource;
use campfire_declare::{Role, Scratch};
use campfire_sim::StableId;
use campfire_values::{ActionId, ModifierId, Rank};

use crate::projection::Projection;

/// The modifiers capabilities other than stats hold on units this tick, each on its unit or on
/// the units a projection selects, which stats hold in `StatsSet::Hold` as they hold auras': each
/// producer adds to it before that set, whose pass takes all it holds, so every producer's
/// entries reach it, and none outlives its tick. Not state.
#[derive(Resource, Debug, Default)]
pub struct HeldModifiers {
    pub(crate) held: Vec<Held>,
    pub(crate) projections: Vec<Projection>,
}

/// A modifier held on a unit: the unit, the modifier, the unit that holds it, none for a
/// player's, and the ability whose params it reads, at its rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Held {
    pub target: StableId,
    pub modifier: ModifierId,
    pub source: Option<StableId>,
    pub action: Option<ActionId>,
    pub rank: Rank,
}

impl HeldModifiers {
    /// Holds `held` on its unit this tick.
    pub fn hold(&mut self, held: Held) {
        self.held.push(held);
    }

    /// Holds the modifier of `projection` this tick on each unit it selects.
    pub fn project(&mut self, projection: Projection) {
        self.projections.push(projection);
    }
}

impl Scratch for HeldModifiers {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }

    fn is_clear(&self) -> bool {
        self.held.is_empty() && self.projections.is_empty()
    }
}
