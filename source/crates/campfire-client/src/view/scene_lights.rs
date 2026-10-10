use std::f32::consts::PI;

use bevy::camera::Exposure;
use bevy::color::{Color, LinearRgba, Srgba};
use bevy::light::{DirectionalLight, GlobalAmbientLight};
use bevy::math::Vec3;
use bevy::transform::components::Transform;
use campfire_package::MapLights;

/// A map's lights as Bevy draws them, so that a light of 1 gives a surface its full color, as a
/// fixed-function renderer's does: the camera's exposure divides every light, and a directional
/// light's diffuse term divides it by π more, so each light's strength is the inverse of both.
/// A game's light colors are added to sRGB codes, so each is an sRGB code itself, which linear
/// light multiplies as the codes do.
#[derive(Debug, Clone)]
pub(crate) struct SceneLights {
    pub(crate) ambient: GlobalAmbientLight,
    pub(crate) suns: Vec<Sun>,
}

/// A directional light, and its transform, which faces the way its light goes.
#[derive(Debug, Clone)]
pub(crate) struct Sun {
    pub(crate) light: DirectionalLight,
    pub(crate) transform: Transform,
}

/// The exposure of the player's camera, which the lights' strengths answer.
pub(crate) const EXPOSURE: Exposure = Exposure::BLENDER;

impl SceneLights {
    /// The lights of `lights`.
    pub(crate) fn of(lights: &MapLights) -> SceneLights {
        let full = 1.0 / EXPOSURE.exposure();
        let color = |[red, green, blue]: [f32; 3]| {
            Color::LinearRgba(LinearRgba::from(Srgba::new(red, green, blue, 1.0)))
        };
        let suns = lights
            .lights
            .iter()
            .map(|sun| {
                let way = Vec3::from_array(sun.direction);
                let up = if way.cross(Vec3::Y) == Vec3::ZERO {
                    Vec3::Z
                } else {
                    Vec3::Y
                };
                Sun {
                    light: DirectionalLight {
                        color: color(sun.color),
                        illuminance: PI * full,
                        ..DirectionalLight::default()
                    },
                    transform: Transform::default().looking_to(way, up),
                }
            })
            .collect();
        SceneLights {
            ambient: GlobalAmbientLight {
                color: color(lights.ambient),
                brightness: full,
                ..GlobalAmbientLight::default()
            },
            suns,
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_package::MapLight;

    use super::*;

    #[test]
    fn a_light_of_1_gives_a_surface_its_full_color() {
        let lights = MapLights {
            ambient: [0.5, 0.0, 1.0],
            lights: vec![
                MapLight {
                    color: [1.0, 0.5, 0.0],
                    direction: [0.0, -1.0, 0.0],
                },
                MapLight {
                    color: [0.25, 0.25, 0.25],
                    direction: [1.0, -1.0, 0.0],
                },
            ],
        };
        let scene = SceneLights::of(&lights);
        // EV100 9.7: an exposure of 2^−9.7 / 1.2, so a full light is 1.2 · 2^9.7 = 998.18.
        let full = 1.2 * 9.7_f32.exp2();
        assert!((scene.ambient.brightness - full).abs() < 1e-2);
        assert!((scene.suns[0].light.illuminance - PI * full).abs() < 1e-1);
        // sRGB code 0.5 is linear 0.21404; 0 and 1 stay.
        let LinearRgba {
            red, green, blue, ..
        } = scene.suns[0].light.color.to_linear();
        assert_eq!((red, blue), (1.0, 0.0));
        assert!((green - 0.214_04).abs() < 1e-5);
        assert_eq!(
            scene.ambient.color.to_linear().red.to_bits(),
            green.to_bits()
        );
        // A light straight down faces −y, its up +z; one aslant faces its way.
        assert!((scene.suns[0].transform.forward().as_vec3() - Vec3::NEG_Y).length() < 1e-6);
        let aslant = Vec3::new(1.0, -1.0, 0.0).normalize();
        assert!((scene.suns[1].transform.forward().as_vec3() - aslant).length() < 1e-6);
    }
}
