use crate::actions::cost_target::CostTarget;
use crate::scripts::error::UnknownName;
use crate::stats::stat_id::StatId;
use crate::units::filter::Filter;
use crate::units::modifier_id::ModifierId;
use crate::units::unit_type::UnitType;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::stat::Stat;

/// The match's ids of the names an action's data gives: what an action's load resolves them by,
/// in a match's world or in the load's builder; an error for a name the match does not have.
pub(crate) trait ActionNames {
    /// The place of `stat` among the match's stats.
    fn stat(&self, stat: &Stat) -> Result<StatId, UnknownName>;
    fn damage_kind(&self, name: &DeclaredName) -> Result<DamageKind, UnknownName>;
    /// What a cost named `name` takes from; none for a name of neither.
    fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget>;
    fn filter(&self, filter: &FilterData) -> Result<Filter, UnknownName>;
    /// The modifier `name` of `package`.
    fn modifier(&self, package: u16, name: &DeclaredName) -> Result<ModifierId, UnknownName>;
    /// The unit type `name` in the scope that `package` names types in.
    fn unit_type(&self, package: u16, name: &DeclaredName) -> Result<UnitType, UnknownName>;
    /// Whether the projectile type `name` of `package`, which resolved, homes on a unit.
    fn homes(&self, package: u16, name: &DeclaredName) -> bool;
}
