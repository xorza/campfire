use super::*;

#[test]
fn the_half_fields_tangent_is_the_nearest_f32_to_its_exact_value() {
    // An `f64` tangent of the `f32` angle errs by far less than half an `f32` step, so it
    // rounds to the nearest `f32`, as the constant is.
    let angle = (FIELD_OF_VIEW / 2.0).to_radians();
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the f64 tangent rounds to its nearest f32"
    )]
    let rounded = f64::from(angle).tan() as f32;
    assert_eq!(TAN_HALF_FIELD, rounded);
    assert_eq!(angle, f32::from_bits(0x3EDF_66F3));
}

#[test]
fn the_shipped_camera_scrolls_turns_and_zooms_at_the_games_speeds() {
    // A key scrolls 100 pixels a frame times 0.5 and the axis's factor, each 250 pixels of a
    // view plane 2 · tan(25°) wide across 800, each step an `f32`: across, 1.6 · 100 · 0.5 ·
    // 250 · plane · 30 = 699.46155 m/s, 0x442E_DD8A; along, at 2.0, 874.3269 m/s,
    // 0x445A_94EC; turning 0.1 rad a frame is 3 rad/s; zooming 10 a frame, 300 a second.
    let mut shipped = GameDataIni::default();
    shipped
            .read(b"GameData\n CameraPitch = 37.5\n MaxCameraHeight = 310.0\n MinCameraHeight = 120.0\n HorizontalScrollSpeedFactor = 1.6;\n VerticalScrollSpeedFactor = 2.0\nEnd\n")
            .unwrap();
    let camera = camera_file(&shipped).unwrap();
    assert_eq!(
        (camera.pitch, camera.yaw, camera.field_of_view),
        (37.5, 0.0, 50.0)
    );
    assert_eq!(
        camera.height,
        CameraHeight {
            least: 120.0,
            most: 310.0,
        }
    );
    assert_eq!(
        camera.scroll,
        Vec2::new(f32::from_bits(0x442E_DD8A), f32::from_bits(0x445A_94EC))
    );
    assert_eq!(camera.turn, 3.0);
    assert_eq!(
        (camera.drag_turn, camera.zoom_notch, camera.zoom_key),
        (0.01, 10.0, 300.0)
    );
    // The edge's 3 pixels.
    assert_eq!(camera.edge, 3.0);
}
