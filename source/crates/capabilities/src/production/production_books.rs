use bevy_ecs::system::{Res, SystemParam};

use crate::actions::action_book::ActionBook;
use crate::production::production_data::ProductionData;
use crate::production::requirement_book::RequirementBook;
use crate::production::supply_book::SupplyBook;
use crate::production::supply_cost_book::SupplyCostBook;
use crate::units::by_type::ByType;

/// The books a train is checked and paid by: the actions, each producer's data, what a train
/// requires, what it costs in supply, and the supply rules, none in a match without supply.
#[derive(SystemParam, Debug)]
pub(crate) struct ProductionBooks<'w> {
    pub(crate) actions: Res<'w, ActionBook>,
    pub(crate) producers: Res<'w, ByType<ProductionData>>,
    pub(crate) requirements: Res<'w, RequirementBook>,
    pub(crate) costs: Res<'w, SupplyCostBook>,
    pub(crate) rules: Res<'w, SupplyBook>,
}
