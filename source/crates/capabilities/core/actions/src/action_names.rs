use campfire_common::DeclaredName;
use campfire_script::{Filter, UnknownName};
use campfire_values::{FilterData, ModifierId, PackageIndex, UnitType};

use crate::cost_target::CostTarget;
use crate::delivery::ProjectileFlight;

/// The match's ids of the names an action's data gives: what an action's load resolves them by,
/// in a match's world or in the load's builder; an error for a name the match does not have.
pub trait ActionNames {
    /// What a cost named `name` takes from; none for a name of neither.
    fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget>;
    fn filter(&self, filter: &FilterData) -> Result<Filter, UnknownName>;
    /// The modifier `name` of `package`.
    fn modifier(
        &self,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<ModifierId, UnknownName>;
    /// The unit type `name` in the scope that `package` names types in.
    fn unit_type(
        &self,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<UnitType, UnknownName>;
    /// How the projectile type `name` of `package`, which resolved, flies; none when it has no
    /// `projectile` section.
    fn flight(&self, package: PackageIndex, name: &DeclaredName) -> Option<ProjectileFlight>;
}
