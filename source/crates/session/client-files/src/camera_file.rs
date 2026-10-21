use bevy_math::Vec2;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;

/// A package's `client/camera.toml`: how the player's camera looks at the map and moves over it,
/// as an RTS's does. It looks down at a point of the ground from a height above it, turned about
/// that point; keys, the screen's edges and the mouse move it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CameraFile {
    /// Degrees below the horizon it looks down at the point in view.
    pub pitch: f32,
    /// Degrees it is turned at the start, counter-clockwise seen from above, from looking north,
    /// the engine's `−z`.
    pub yaw: f32,
    /// Its horizontal field of view, in degrees.
    pub field_of_view: f32,
    /// Its height above the point in view, in meters, which it starts at the most of.
    pub height: CameraHeight,
    /// Meters a second a key or the screen's edge scrolls it across the screen and along it.
    pub scroll: Vec2,
    /// Radians a second a key turns it.
    pub turn: f32,
    /// Radians a pixel of a drag turns it.
    pub drag_turn: f32,
    /// Meters of height a notch of the wheel zooms it.
    pub zoom_notch: f32,
    /// Meters of height a second of a key zooms it.
    pub zoom_key: f32,
    /// Pixels from the window's edge within which the cursor scrolls it.
    pub edge: f32,
}

/// The least and the most height of a camera.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraHeight {
    pub least: f32,
    pub most: f32,
}

impl CameraFile {
    /// Where a package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/camera.toml")
    }

    /// The camera, unless a value is one no camera takes: a pitch not above 0° or not below 90°,
    /// a field of view not above 0° or not below 180°, a least height not above 0 or past the
    /// most, a yaw, a speed or an edge not finite, or one of them negative.
    pub fn checked(self) -> Option<Self> {
        self.holds().then_some(self)
    }

    fn holds(&self) -> bool {
        let speeds = [
            self.scroll.x,
            self.scroll.y,
            self.turn,
            self.drag_turn,
            self.zoom_notch,
            self.zoom_key,
            self.edge,
        ];
        self.pitch > 0.0
            && self.pitch < 90.0
            && self.field_of_view > 0.0
            && self.field_of_view < 180.0
            && self.yaw.is_finite()
            && self.height.least > 0.0
            && self.height.least <= self.height.most
            && self.height.most.is_finite()
            && speeds
                .iter()
                .all(|speed| speed.is_finite() && *speed >= 0.0)
    }
}

/// A package's file is untrusted, so a camera whose values no camera takes fails to read.
impl<'de> Deserialize<'de> for CameraFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            pitch: f32,
            yaw: f32,
            field_of_view: f32,
            height: CameraHeight,
            scroll: Vec2,
            turn: f32,
            drag_turn: f32,
            zoom_notch: f32,
            zoom_key: f32,
            edge: f32,
        }
        let Fields {
            pitch,
            yaw,
            field_of_view,
            height,
            scroll,
            turn,
            drag_turn,
            zoom_notch,
            zoom_key,
            edge,
        } = Fields::deserialize(deserializer)?;
        let file = Self {
            pitch,
            yaw,
            field_of_view,
            height,
            scroll,
            turn,
            drag_turn,
            zoom_notch,
            zoom_key,
            edge,
        };
        if file.holds() {
            Ok(file)
        } else {
            Err(D::Error::custom(
                "a camera's pitch, field of view, heights or speeds are out of range",
            ))
        }
    }
}

impl ClientFile for CameraFile {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_camera_reads_only_with_values_a_camera_takes() {
        let text = |pitch: &str, least: &str| {
            format!(
                "pitch = {pitch}\nyaw = 0.0\nfield_of_view = 50.0\nheight = {{ least = {least}, most = 310.0 }}\nscroll = [700.0, 875.0]\nturn = 3.0\ndrag_turn = 0.01\nzoom_notch = 10.0\nzoom_key = 300.0\nedge = 3.0\n"
            )
        };
        let file = Toml::parse::<CameraFile>(&text("37.5", "120.0")).unwrap();
        assert_eq!(
            file.height,
            CameraHeight {
                least: 120.0,
                most: 310.0,
            }
        );
        assert_eq!(file.edge, 3.0);
        assert_eq!(
            Toml::parse::<CameraFile>(&Toml::write(&file)).unwrap(),
            file
        );
        // A negative edge is no camera, nor a file that gives none, nor one that gives a click's
        // time, which is the press's of `client/keys.toml`.
        let shipped = text("37.5", "120.0");
        let refusal = |text: &str| {
            let refused = Toml::parse::<CameraFile>(text).unwrap_err();
            refused.message().to_owned()
        };
        let out_of_range = "a camera's pitch, field of view, heights or speeds are out of range";
        assert_eq!(
            refusal(&shipped.replace("edge = 3.0", "edge = -1.0")),
            out_of_range
        );
        assert_eq!(
            refusal(&shipped.replace("edge = 3.0\n", "")),
            "missing field `edge`"
        );
        let click = refusal(&shipped.replace("edge = 3.0\n", "edge = 3.0\nclick = 0.167\n"));
        assert!(click.starts_with("unknown field `click`"), "{click}");
        // Looking level or straight down, or a least height past the most or not above 0, is no
        // camera.
        for (pitch, least) in [
            ("0.0", "120.0"),
            ("90.0", "120.0"),
            ("37.5", "320.0"),
            ("37.5", "0.0"),
        ] {
            assert_eq!(
                refusal(&text(pitch, least)),
                out_of_range,
                "{pitch} {least}"
            );
        }
    }
}
