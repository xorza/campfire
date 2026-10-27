#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a call does with the client's camera: reads its rig, sets it, or turns it to look at a
/// unit or at a point of the ground.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CameraAsk {
    Read,
    Set(CameraPose),
    /// Looks at the unit of this stable id.
    Unit(u64),
    /// Looks at the point of the ground plane `x`, `z`.
    Point {
        x: f32,
        z: f32,
    },
}

/// The client's camera rig: it looks down at `pitch` radians at the point `pivot` of the
/// ground, `[x, z]`, from `height` metres above it, turned by `yaw` radians counter-clockwise
/// seen from above from looking north, the engine's −z.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CameraPose {
    pub pivot: [f32; 2],
    pub yaw: f32,
    pub pitch: f32,
    pub height: f32,
}
