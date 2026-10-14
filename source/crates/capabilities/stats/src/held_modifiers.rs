use bevy_ecs::resource::Resource;
use campfire_declare::Scratch;
use campfire_sim::StableId;
use campfire_values::{ActionId, ModifierId, Rank};

/// The modifiers capabilities other than stats hold on units this tick, which stats hold in
/// `Phase::HoldModifiers` as they hold auras': each producer adds to it before that phase, whose
/// pass takes all it holds, so every producer's entries reach it, and none outlives its tick. Not
/// state.
#[derive(Resource, Debug, Default)]
pub struct HeldModifiers(pub Vec<Held>);

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

impl Scratch for HeldModifiers {
    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
