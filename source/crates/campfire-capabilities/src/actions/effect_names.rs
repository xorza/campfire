use crate::stats::pool_id::PoolId;
use crate::units::modifier_id::ModifierId;
use crate::units::tag::Tag;
use crate::units::track_id::TrackId;
use crate::units::unit_type::UnitType;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;

/// The match's ids of the names an action's effect lists give, which the package load checked:
/// what an action's lists resolve them by, in a match's world or in the load's builder.
pub(crate) trait EffectNames {
    /// The place among the action's params of the param `name`.
    fn param(&self, name: &DeclaredName) -> usize;
    fn damage_kind(&self, name: &DeclaredName) -> DamageKind;
    fn pool(&self, name: &DeclaredName) -> PoolId;
    /// The modifier `name` of the action's package.
    fn modifier(&self, name: &DeclaredName) -> ModifierId;
    fn track(&self, name: &DeclaredName) -> TrackId;
    fn tag(&self, name: &DeclaredName) -> Tag;
    /// The projectile or area type `name` of the action's package.
    fn delivery_type(&self, name: &DeclaredName) -> UnitType;
    /// The mode's unit type `name` that stands, which a spawn makes.
    fn standing_type(&self, name: &DeclaredName) -> UnitType;
}
