use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops a particle system, a part of one, a nugget of an effect list, or a part
/// of an object's death or its draw's marks on the ground (effects.md, What the slice leaves;
/// effects.md, The import hands the client consistent effects).
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
    /// A list's nugget of a sound.
    Sound,
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
    /// An object's slow death creates objects in a phase, as a hulk or thrown debris.
    DeathObject,
    /// An object's slow death fires a weapon in a phase.
    DeathWeapon,
    /// An object's draw throws tread dust as it moves.
    TreadDust,
    /// An object's draw lays track marks on the terrain as it moves.
    TrackMarks,
}
