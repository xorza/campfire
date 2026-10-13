use bevy_ecs::resource::Resource;
use campfire_core::{Scratch, UnitType};
use campfire_deliveries::Delivering;
use campfire_sim::{Position, StableId};

/// The areas that land this tick: those actions deliver, and those scripts place. Not state: it
/// empties within the tick.
#[derive(Resource, Debug, Default)]
pub struct AreaLaunches(pub(crate) Vec<AreaLaunch>);

/// An area of `unit_type` of `by` that lands at `at`, aimed at `aimed`, if at a unit; with `id`,
/// the id the script that placed it took for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AreaLaunch {
    pub id: Option<StableId>,
    pub(crate) by: Delivering,
    pub pos: Position,
    pub(crate) unit_type: UnitType,
    pub(crate) aimed: Option<StableId>,
}

impl Scratch for AreaLaunches {
    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
