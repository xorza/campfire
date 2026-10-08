use bevy_ecs::resource::Resource;
use campfire_sim::StableId;

use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::values::rank::Rank;

/// The modifiers capabilities other than stats hold on units this tick, which `StatsSet::Hold`
/// holds as it holds auras': each producer adds to it before that set, which takes all it holds,
/// so every producer's entries reach it, and none outlives its tick. Not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct HeldModifiers(pub(crate) Vec<Held>);

/// A modifier held on a unit: the unit, the modifier, the unit that holds it, none for a
/// player's, and the ability whose params it reads, at its rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Held {
    pub(crate) target: StableId,
    pub(crate) modifier: ModifierId,
    pub(crate) source: Option<StableId>,
    pub(crate) ability: Option<ActionId>,
    pub(crate) rank: Rank,
}
