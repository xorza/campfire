use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops a particle system, a part of one, or a nugget of an effect list
/// (effects.md, What the slice leaves; effects.md, The import hands the client consistent
/// effects).
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
}
