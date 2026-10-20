//! The player's camera of the imported package, `client/camera.toml`, as the game's tactical view
//! moves it, from what its `GameData.ini` and `Mouse.ini` files give.

use bevy_math::Vec2;
use campfire_client_files::{CameraFile, CameraHeight};

use crate::zero_hour::format::ini::game_data_ini::GameDataIni;

/// The game's frames a second, which its camera's speeds count by.
const FRAMES: f32 = 30.0;
/// The view's horizontal field of view, in degrees, which `View` fixes.
const FIELD_OF_VIEW: f32 = 50.0;
/// The tangent of half the field of view, which a camera's speeds are in the import's
/// fingerprint by, so no OS's own `tan` gives it: 25° as an `f32` of radians is
/// 0.436332315…, whose tangent 0.466307660843… rounds to the `f32` 0x3EEE_BFE1, 0.466307669…
const TAN_HALF_FIELD: f32 = f32::from_bits(0x3EEE_BFE1);
/// The pixels a key scrolls a frame, before its factors: retail's `SCROLL_AMT`.
const SCROLL_PIXELS: f32 = 100.0;
/// `W3DView::scrollBy`'s screen pixels for each pixel of a scroll.
const SCROLL_RESOLUTION: f32 = 250.0;
/// The display width, retail's 800 × 600, at which a scroll's pixels give the speeds the camera
/// file keeps: `scrollBy` measures them on the view plane, which a wider display divides finer.
const DISPLAY_WIDTH: f32 = 800.0;
/// The height a notch of the wheel, or a frame of a zoom key, zooms: `ZoomHeightPerSecond`.
const ZOOM_HEIGHT: f32 = 10.0;
/// Radians a pixel of a middle-button drag turns the view.
const DRAG_TURN: f32 = 0.01;
/// The pixels from the screen's edge within which the cursor scrolls: `LookAtTranslator`'s
/// `edgeScrollSize`.
const EDGE: f32 = 3.0;

/// The player's camera of `data`, as the game's tactical view moves it at its start, its zoom at
/// its most: a key or the screen's edge scrolls it `SCROLL_PIXELS` a frame times the axis's factor and the default
/// scroll factor, each pixel `SCROLL_RESOLUTION` pixels of the view plane, whose width at depth 1
/// is `2 · tan(fov / 2)` across `DISPLAY_WIDTH` pixels.
///
/// The pitch is clamped to 0.1°–89.9° and the most height raised to the least, as the view takes
/// them; none for a camera the client takes no other way.
pub(crate) fn camera_file(data: &GameDataIni) -> Option<CameraFile> {
    let plane = 2.0 * TAN_HALF_FIELD / DISPLAY_WIDTH;
    let speed = |factor: f32| {
        factor * SCROLL_PIXELS * data.default_scroll * SCROLL_RESOLUTION * plane * FRAMES
    };
    CameraFile {
        pitch: data.camera_pitch.clamp(0.1, 89.9),
        yaw: data.camera_yaw,
        field_of_view: FIELD_OF_VIEW,
        height: CameraHeight {
            least: data.min_camera_height,
            most: data.max_camera_height.max(data.min_camera_height),
        },
        scroll: Vec2::new(speed(data.horizontal_scroll), speed(data.vertical_scroll)),
        turn: data.rotate_speed * FRAMES,
        drag_turn: DRAG_TURN,
        zoom_notch: ZOOM_HEIGHT,
        zoom_key: ZOOM_HEIGHT * FRAMES,
        edge: EDGE,
    }
    .checked()
}

#[cfg(test)]
mod tests {
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
}
