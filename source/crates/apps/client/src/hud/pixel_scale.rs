use bevy::camera::Projection;
use bevy::math::{Mat4, Vec3, Vec4};
use bevy::transform::components::Transform;

/// How the camera shows the world on a screen: its place, its projection, and the screen's
/// height in logical pixels, by which a stand's unit is made a pixel at the stand's depth
/// (02-engine-core.md, Bars in screen space).
#[derive(Debug, Clone, Copy)]
pub(crate) struct PixelScale {
    eye: Transform,
    clip_from_view: Mat4,
    height: f32,
}

impl PixelScale {
    pub(crate) fn of(eye: Transform, projection: &Projection, height: f32) -> Self {
        Self {
            eye,
            clip_from_view: projection.get_clip_from_view(),
            height,
        }
    }

    /// The meters a pixel of the screen spans at `point`, by the depth of `point` before the
    /// camera: what one meter up at that depth moves on the screen, as its projection gives it;
    /// none for a point the camera does not show, behind it or on its plane.
    pub(crate) fn meters_at(&self, point: Vec3) -> Option<f32> {
        let depth = (point - self.eye.translation).dot(*self.eye.forward());
        let ndc = |up: f32| {
            let clip = self.clip_from_view * Vec4::new(0.0, up, -depth, 1.0);
            clip.y / clip.w
        };
        let meters = 2.0 / ((ndc(1.0) - ndc(0.0)) * self.height);
        (depth > 0.0 && meters.is_finite() && meters > 0.0).then_some(meters)
    }
}

#[cfg(test)]
mod tests {
    use bevy::camera::{
        CameraProjection, OrthographicProjection, PerspectiveProjection, ScalingMode,
    };

    use super::*;

    #[test]
    fn a_pixel_spans_the_view_s_height_at_its_depth_over_the_screen_s_pixels() {
        // A view 90° high: at a depth of 10 m it is 2 · 10 · tan 45° = 20 m high, over 720
        // pixels, so a pixel spans 20 / 720 m, within the rounding of tan 45° in f32. Twice as
        // deep, twice as far.
        let perspective = Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            ..PerspectiveProjection::default()
        });
        let eye = Transform::from_xyz(0.0, 0.0, 10.0);
        let scale = PixelScale::of(eye, &perspective, 720.0);
        let near = |got: Option<f32>, want: f32| got.is_some_and(|got| (got - want).abs() < 1e-7);
        assert!(near(scale.meters_at(Vec3::ZERO), 20.0 / 720.0));
        assert!(near(
            scale.meters_at(Vec3::new(3.0, 1.0, -10.0)),
            40.0 / 720.0
        ));
        // A point behind the camera, or on its plane, shows on no pixel.
        assert_eq!(scale.meters_at(Vec3::new(0.0, 0.0, 12.0)), None);
        assert_eq!(scale.meters_at(Vec3::new(5.0, 0.0, 10.0)), None);
        // A view 300 m high of no depth, over 600 pixels: half a meter at any depth.
        let orthographic = Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed {
                width: 400.0,
                height: 300.0,
            },
            ..OrthographicProjection::default_3d()
        });
        let mut projection = orthographic;
        if let Projection::Orthographic(ortho) = &mut projection {
            ortho.update(400.0, 300.0);
        }
        let scale = PixelScale::of(eye, &projection, 600.0);
        assert!(near(scale.meters_at(Vec3::ZERO), 0.5));
        assert!(near(scale.meters_at(Vec3::new(0.0, 0.0, -90.0)), 0.5));
    }
}
