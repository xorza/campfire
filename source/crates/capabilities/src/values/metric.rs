use bevy_ecs::resource::Resource;
use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use serde::Deserialize;

/// How a map measures ranges, reach and sight: on the ground plane, as MOBAs and RTS games do, or
/// in 3D, as shooters and flight do. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    #[default]
    Planar,
    Spatial,
}

impl Metric {
    /// The offset from `a` to `b` that counts towards a distance: a planar map drops the height.
    pub(crate) fn offset(self, a: Position, b: Position) -> Vec3 {
        match self {
            Metric::Planar => a.ground_offset(b),
            Metric::Spatial => b.get() - a.get(),
        }
    }

    /// Whether `b` is within `range` of `a`, exactly, with no square root.
    pub(crate) fn within(self, a: Position, b: Position, range: Num) -> bool {
        Vec3::ZERO.within(self.offset(a, b), range)
    }
}
