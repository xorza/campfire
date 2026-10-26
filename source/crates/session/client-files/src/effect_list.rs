use bevy_math::{Quat, Vec3};
use campfire_common::DeclaredName;
use serde::{Deserialize, Serialize};

use crate::spread::Spread;

/// An effect a weapon or a death plays: the particle systems it starts, each as its entry says,
/// in their order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectList {
    pub systems: Vec<ListedSystem>,
}

/// A particle system an effect starts, `count` times: each at the effect's point plus `offset`,
/// turned as the effect's source when `orient_to_object`, plus a distance of `radius` at an angle
/// drawn about the vertical, and `height` above, or at the ground's height when `at_ground`;
/// turned by `rotation`; on the effect's object, moving with it, when `attach_to_object`; after
/// `delay` steps, rounded up, when it has one and its draw is not below 0, else after the
/// system's own; its emission volume's
/// radius the caller's when `callers_radius` and the caller gives one; and, when `ricochet`,
/// turned along the line from the effect's second object to its first. Distances are world units
/// on the engine's axes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListedSystem {
    pub system: DeclaredName,
    pub count: u32,
    pub offset: Vec3,
    pub radius: Spread,
    pub height: Spread,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<Spread>,
    pub rotation: Quat,
    pub orient_to_object: bool,
    pub attach_to_object: bool,
    pub at_ground: bool,
    pub callers_radius: bool,
    pub ricochet: bool,
}

impl ListedSystem {
    /// Whether each of its numbers is finite.
    pub(crate) fn is_finite(&self) -> bool {
        self.offset.is_finite()
            && self.radius.is_finite()
            && self.height.is_finite()
            && self.delay.is_none_or(Spread::is_finite)
            && self.rotation.is_finite()
    }
}
