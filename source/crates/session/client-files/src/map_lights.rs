use bevy_math::Vec3;
use campfire_common::MapName;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_path;

/// A map's `client/maps/<name>/lights.toml`: the light its units are drawn in, as a fixed-function
/// renderer lights them: the ambient color, and each directional light's color, added as it meets
/// a surface's normal. Colors are a game's numbers, 1 a surface's full color, as it adds them to
/// its colors; a direction is the way the light goes, on the engine's axes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MapLights {
    pub ambient: [f32; 3],
    pub lights: Vec<MapLight>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapLight {
    pub color: [f32; 3],
    pub direction: Vec3,
}

impl MapLights {
    /// The lights of `ambient` and `lights`; none for a number that is not finite, or a light
    /// whose direction has no length, which no directional light has.
    pub fn new(ambient: [f32; 3], lights: impl IntoIterator<Item = MapLight>) -> Option<Self> {
        let lights: Vec<MapLight> = lights.into_iter().collect();
        let finite = |values: [f32; 3]| values.iter().all(|value| value.is_finite());
        let holds = finite(ambient)
            && lights.iter().all(|light| {
                finite(light.color) && light.direction.is_finite() && light.direction != Vec3::ZERO
            });
        holds.then_some(Self { ambient, lights })
    }

    /// The file of the map `map`.
    pub fn path(map: &MapName) -> PackagePath {
        client_path::of_map(map, "lights.toml")
    }
}

/// A package's file is untrusted, so lights of a number that is not finite, or a direction of no
/// length, fail to read.
impl<'de> Deserialize<'de> for MapLights {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            ambient: [f32; 3],
            lights: Vec<MapLight>,
        }
        let Fields { ambient, lights } = Fields::deserialize(deserializer)?;
        Self::new(ambient, lights).ok_or_else(|| {
            D::Error::custom("a light's number is not finite, or its direction has no length")
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn lights_read_only_with_finite_numbers_and_a_direction() {
        let text = |direction: &str| {
            format!(
                "ambient = [0.2, 0.2, 0.25]\n[[lights]]\ncolor = [0.8, 0.7, 0.6]\ndirection = {direction}\n"
            )
        };
        let lights = Toml::parse::<MapLights>(&text("[-0.5, -0.7, 0.5]")).unwrap();
        assert_eq!(
            lights.lights,
            [MapLight {
                color: [0.8, 0.7, 0.6],
                direction: Vec3::new(-0.5, -0.7, 0.5),
            }]
        );
        assert_eq!(
            Toml::parse::<MapLights>(&Toml::write(&lights)).unwrap(),
            lights
        );
        assert!(Toml::parse::<MapLights>(&text("[0.0, 0.0, 0.0]")).is_err());
        assert!(Toml::parse::<MapLights>(&text("[0.0, nan, 0.0]")).is_err());
    }
}
