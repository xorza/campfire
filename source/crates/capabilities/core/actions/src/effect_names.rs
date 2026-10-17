use campfire_common::DeclaredName;
use campfire_script::UnknownName;
use campfire_stats::PoolId;
use campfire_values::{DamageKind, ModifierId, Tag, TrackId, UnitType};

/// The match's ids of the names an action's effect lists give: what an action's lists resolve
/// them by, in a match's world or in the load's builder; an error for a name the match does not
/// have.
pub trait EffectNames {
    /// The place among the action's params of the param `name`.
    fn param(&self, name: &DeclaredName) -> Result<usize, UnknownName>;
    fn damage_kind(&self, name: &DeclaredName) -> Result<DamageKind, UnknownName>;
    fn pool(&self, name: &DeclaredName) -> Result<PoolId, UnknownName>;
    /// The modifier `name` of the action's package.
    fn modifier(&self, name: &DeclaredName) -> Result<ModifierId, UnknownName>;
    fn track(&self, name: &DeclaredName) -> Result<TrackId, UnknownName>;
    fn tag(&self, name: &DeclaredName) -> Result<Tag, UnknownName>;
    /// The unit type `name` of the action's package: a launch's area type, or one a spawn makes.
    fn package_type(&self, name: &DeclaredName) -> Result<UnitType, UnknownName>;
}
