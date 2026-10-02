use crate::actions::action_data::CostTarget;
use crate::stats::modifier_book::ModifierId;
use crate::stats::stat_id::StatId;
use crate::units::filter::Filter;
use crate::units::unit_type::UnitType;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::stat::Stat;

/// The match's ids of the names an action's data gives, which the package load checked: what an
/// action's load resolves them by, in a match's world or in the load's builder.
pub(crate) trait ActionNames {
    /// The place of `stat` among the match's stats.
    fn stat(&self, stat: &Stat) -> StatId;
    fn damage_kind(&self, name: &DeclaredName) -> DamageKind;
    /// What a cost named `name` takes from.
    fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget>;
    fn filter(&self, filter: &FilterData) -> Filter;
    /// The modifier `name` of `package`.
    fn modifier(&self, package: u16, name: &DeclaredName) -> ModifierId;
    /// The unit type `name` in the scope that `package` names types in.
    fn unit_type(&self, package: u16, name: &DeclaredName) -> UnitType;
    /// Whether the projectile type `name` of `package` homes on a unit.
    fn homes(&self, package: u16, name: &DeclaredName) -> bool;
}
