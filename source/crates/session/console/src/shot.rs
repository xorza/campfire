#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::camera_ask::CameraPose;

/// A screenshot: a PNG of `width` × `height` pixels, as base64, of the region of the window's
/// frame from its pixel `origin`, scaled by `scale` image pixels a frame pixel, and the camera's
/// rig it drew by (console.md, A pixel is the screenshot's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WindowShot {
    pub png: String,
    pub width: u32,
    pub height: u32,
    pub origin: [u32; 2],
    pub scale: f32,
    pub camera: CameraPose,
}
