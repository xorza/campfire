use bevy::camera::{CameraProjection, OrthographicProjection, PerspectiveProjection, ScalingMode};

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
