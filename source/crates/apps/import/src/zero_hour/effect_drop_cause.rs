use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops a particle system, a part of one, a nugget of an effect list, or a part
/// of an object's death or its draw's marks on the ground (09-zero-hour.md, Particles and effects;
/// 09-zero-hour.md, The import hands the client consistent effects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum EffectDropCause {
    /// A system of streak, volume or drawable particles, or of none, which draws nothing.
    Kind,
    /// A system of no shader, which the game draws in the shader drawn before it.
    Shader,
    /// A system whose texture no archive holds.
    Texture,
    /// A system's wind motion, which it moves without.
    Wind,
    /// A list's nugget, a system's slave or its particles' system, of a name no system has.
    NoSystem,
    /// A list's nugget of a ray effect.
    RayEffect,
    /// A list's nugget of a tracer.
    Tracer,
    /// A list's nugget of a pulse of light.
    LightPulse,
    /// A list's nugget that shakes the view.
    ViewShake,
    /// A list's nugget of a scorch on the terrain.
    TerrainScorch,
    /// A list's nugget of a list played at a model's bones.
    AtBone,
    /// An object's slow death names a phase's FX lists past its first, of which the game draws
    /// one at random and the import takes the first.
    DeathChoice,
    /// An object's slow death fires a weapon in a phase.
    DeathWeapon,
    /// A death names an object creation list no list holds.
    NoCreationList,
    /// A creation list makes an object of gameplay, no hulk.
    CreationObject,
    /// A creation list's nugget delivers a payload, applies a force, fires a weapon or attacks.
    CreationNugget,
    /// A wreck's model writes nothing.
    WreckModel,
    /// A wreck floats, whirls or inherits its unit's velocity, which its launch leaves.
    WreckLaunch,
    /// An aircraft's fall plays a jet's secondary, a helicopter's blade or its pilot's ejection.
    FallMoment,
    /// An aircraft's slow death names an FX list, an object creation list or a weapon of a phase,
    /// which its fall leaves.
    FallPhase,
}
