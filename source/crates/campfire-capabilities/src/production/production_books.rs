use bevy_ecs::system::{Res, SystemParam};

use crate::actions::action_book::ActionBook;
use crate::production::production_data::ProductionData;
use crate::production::requirements::Requirements;
use crate::production::supply_costs::SupplyCosts;
use crate::production::supply_rules::SupplyRules;
use crate::units::by_type::ByType;

/// The books a train is checked and paid by: the actions, each producer's data, what a train
/// requires, what it costs in supply, and the supply rules, none in a match without supply.
#[derive(SystemParam, Debug)]
pub(crate) struct ProductionBooks<'w> {
    pub(crate) actions: Res<'w, ActionBook>,
    pub(crate) producers: Res<'w, ByType<ProductionData>>,
    pub(crate) requirements: Res<'w, Requirements>,
    pub(crate) costs: Res<'w, SupplyCosts>,
    pub(crate) rules: Option<Res<'w, SupplyRules>>,
}
