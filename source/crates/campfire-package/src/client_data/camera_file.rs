use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

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
    pub scroll: [f32; 2],
    /// Radians a second a key turns it.
    pub turn: f32,
    /// Radians a pixel of a drag turns it.
    pub drag_turn: f32,
    /// Meters of height a notch of the wheel zooms it.
    pub zoom_notch: f32,
    /// Meters of height a second of a key zooms it.
    pub zoom_key: f32,
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
    pub const PATH: &'static str = "client/camera.toml";

    /// The camera, unless a value is one no camera takes: a pitch not above 0° or not below 90°,
    /// a field of view not above 0° or not below 180°, a least height not above 0 or past the
    /// most, a yaw or a speed not finite, or a negative speed.
    pub fn checked(self) -> Option<CameraFile> {
        self.holds().then_some(self)
    }

    fn holds(&self) -> bool {
        let speeds = [
            self.scroll[0],
            self.scroll[1],
            self.turn,
            self.drag_turn,
            self.zoom_notch,
            self.zoom_key,
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
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CameraFile, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            pitch: f32,
            yaw: f32,
            field_of_view: f32,
            height: CameraHeight,
            scroll: [f32; 2],
            turn: f32,
            drag_turn: f32,
            zoom_notch: f32,
            zoom_key: f32,
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
        } = Fields::deserialize(deserializer)?;
        let file = CameraFile {
            pitch,
            yaw,
            field_of_view,
            height,
            scroll,
            turn,
            drag_turn,
            zoom_notch,
            zoom_key,
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

#[cfg(test)]
mod tests {
    use campfire_common::Toml;

    use super::*;

    #[test]
    fn a_camera_reads_only_with_values_a_camera_takes() {
        let text = |pitch: &str, least: &str| {
            format!(
                "pitch = {pitch}\nyaw = 0.0\nfield_of_view = 50.0\nheight = {{ least = {least}, most = 310.0 }}\nscroll = [700.0, 875.0]\nturn = 3.0\ndrag_turn = 0.01\nzoom_notch = 10.0\nzoom_key = 300.0\n"
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
        assert_eq!(
            Toml::parse::<CameraFile>(&Toml::write(&file).unwrap()).unwrap(),
            file
        );
        // Looking level or straight down, or a least height past the most or not above 0, is no
        // camera.
        for (pitch, least) in [
            ("0.0", "120.0"),
            ("90.0", "120.0"),
            ("37.5", "320.0"),
            ("37.5", "0.0"),
        ] {
            assert!(
                Toml::parse::<CameraFile>(&text(pitch, least)).is_err(),
                "{pitch} {least}"
            );
        }
    }
}
