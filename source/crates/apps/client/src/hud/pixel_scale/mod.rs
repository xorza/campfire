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
mod tests;
