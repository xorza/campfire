use campfire_common::DeclaredName;
use campfire_names::{Filter, NameTables, TagNames, UnknownName};
use campfire_units::PoolId;
use campfire_values::{
    FilterData, NameId, NameKey, Named, Names, PackageIndex, ResourceId, TypeScope, UnitType,
};

use crate::cost_target::CostTarget;
use crate::delivery::{DeliveryAim, DeliveryAims};

/// The match's ids of the names an action's data gives, by the match's name tables, its tags, and
/// how its projectile types fly: what an action's load resolves them by, in the load or in a
/// match's world; an error for a name the match does not have.
#[derive(Debug, Clone, Copy)]
pub struct ActionNames<'a> {
    tables: &'a NameTables,
    tags: &'a TagNames,
    /// How each projectile type flies; none in a match with no projectiles.
    aims: Option<&'a DeliveryAims>,
}

impl<'a> ActionNames<'a> {
    pub const fn new(
        tables: &'a NameTables,
        tags: &'a TagNames,
        aims: Option<&'a DeliveryAims>,
    ) -> Self {
        Self { tables, tags, aims }
    }

    /// The id of type `I` named `key`.
    pub fn named<I: NameId + Named + 'static>(
        &self,
        key: impl NameKey<I::Scope>,
    ) -> Result<I, UnknownName> {
        let id = self.tables.named::<I>(key);
        id.ok_or_else(|| UnknownName::new(I::NAMES, key.text()))
    }

    /// What a cost named `name` takes from; none for a name of neither.
    pub fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget> {
        let none = Names::default();
        let pools = self
            .tables
            .table::<PoolId>()
            .map_or(&none, |pools| &**pools);
        let no_resources = Names::default();
        let resources = self.tables.table::<ResourceId>();
        let resources = resources.map_or(&no_resources, |resources| &**resources);
        CostTarget::named(pools, resources, name.as_str())
    }

    pub fn filter(&self, filter: &FilterData) -> Result<Filter, UnknownName> {
        Filter::resolve(filter, self.tags)
    }

    /// How the delivery type of `unit_type` aims; none when no module delivers it.
    pub fn aim(&self, unit_type: UnitType) -> Option<DeliveryAim> {
        self.aims?.0.get(unit_type).copied()
    }

    /// The unit type `name` in the scope that `package` names types in.
    pub fn unit_type(
        &self,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<UnitType, UnknownName> {
        let scope = TypeScope::of_package(package);
        self.named::<UnitType>((scope, name.as_str()))
    }
}
