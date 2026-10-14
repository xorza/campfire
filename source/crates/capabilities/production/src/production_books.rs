use bevy_ecs::system::{Res, SystemParam};
use campfire_actions::ActionBook;
use campfire_declare::ByType;

use crate::production_data::ProductionData;
use crate::requirement_book::RequirementBook;
use crate::supply_book::SupplyBook;
use crate::supply_cost_book::SupplyCostBook;

/// The books a train is checked and paid by: the actions, each producer's data, what a train
/// requires, what it costs in supply, and the supply rules, none in a match without supply.
#[derive(SystemParam, Debug)]
pub(crate) struct ProductionBooks<'w> {
    pub actions: Res<'w, ActionBook>,
    pub(crate) producers: Res<'w, ByType<ProductionData>>,
    pub(crate) requirements: Res<'w, RequirementBook>,
    pub(crate) costs: Res<'w, SupplyCostBook>,
    pub(crate) rules: Res<'w, SupplyBook>,
}
