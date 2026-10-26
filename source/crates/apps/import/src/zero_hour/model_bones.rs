use campfire_combat::{BarrelData, UnitOffset};

/// What a unit's turret and weapons read of its model at rest (04-capabilities/combat.md, A shot
/// leaves its weapon's barrel): the points of its unit's frame its turret turns and pitches
/// about, and each weapon slot's barrels, of its first draw module of a model, as
/// `Drawable::getProjectileLaunchOffset` reads the first module that draws one, its default
/// state's model at its object's scale.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ModelBones {
    pub(crate) pivot: Option<UnitOffset>,
    pub(crate) pitch_pivot: Option<UnitOffset>,
    pub(crate) barrels: [Vec<BarrelData>; 3],
}
