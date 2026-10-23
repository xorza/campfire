//! The player's camera of the imported package, `client/camera.toml`, as the game's tactical view
//! moves it, from what its `GameData.ini` and `Mouse.ini` files give.

use bevy_math::Vec2;
use campfire_client_files::{CameraFile, CameraHeight};

use crate::zero_hour::format::ini::game_data_ini::GameDataIni;
use crate::zero_hour::game_time;

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
        factor
            * SCROLL_PIXELS
            * data.default_scroll
            * SCROLL_RESOLUTION
            * plane
            * game_time::FRAMES_PER_SECOND
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
        turn: data.rotate_speed * game_time::FRAMES_PER_SECOND,
        drag_turn: DRAG_TURN,
        zoom_notch: ZOOM_HEIGHT,
        zoom_key: ZOOM_HEIGHT * game_time::FRAMES_PER_SECOND,
        edge: EDGE,
    }
    .checked()
}

#[cfg(test)]
mod tests;
