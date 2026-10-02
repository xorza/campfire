use bevy_ecs::resource::Resource;
use campfire_sim::{Position, StableId};

use crate::deliveries::delivering::Delivering;
use crate::units::unit_type::UnitType;

/// The areas that land this tick: those actions deliver, and those scripts place. Not state: it
/// empties within the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct AreaLaunches(pub(crate) Vec<AreaLaunch>);

/// An area of `unit_type` of `by` that lands at `at`, aimed at `aimed`, if at a unit; with `id`,
/// the id the script that placed it took for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AreaLaunch {
    pub(crate) id: Option<StableId>,
    pub(crate) by: Delivering,
    pub(crate) at: Position,
    pub(crate) unit_type: UnitType,
    pub(crate) aimed: Option<StableId>,
}
