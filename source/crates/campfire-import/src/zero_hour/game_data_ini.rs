use campfire_package::{CameraFile, CameraHeight, GameData};

use crate::zero_hour::error::IniError;
use crate::zero_hour::ini_text::{IniLine, IniText};

/// What the import reads of Zero Hour's `GameData.ini` files, as `GlobalData` reads them: each
/// field from the game's default until a file sets it, a later file's value replacing an earlier
/// one.
#[derive(Debug)]
pub(crate) struct GameDataIni {
    adjust_cliff_textures: bool,
    three_way_blends: i32,
    /// `CameraPitch` and `CameraYaw`, in degrees.
    camera_pitch: f32,
    camera_yaw: f32,
    min_camera_height: f32,
    max_camera_height: f32,
    horizontal_scroll: f32,
    vertical_scroll: f32,
    /// `KeyboardDefaultScrollSpeedFactor`: the player's scroll factor until their options set one.
    default_scroll: f32,
    /// `KeyboardCameraRotateSpeed`: radians a frame of the game's 30.
    rotate_speed: f32,
}

/// The game's frames a second, which its camera's speeds count by.
const FRAMES: f32 = 30.0;
/// The view's horizontal field of view, in degrees, which `View` fixes.
const FIELD_OF_VIEW: f32 = 50.0;
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

impl Default for GameDataIni {
    /// The game's defaults.
    fn default() -> GameDataIni {
        GameDataIni {
            adjust_cliff_textures: false,
            three_way_blends: 1,
            camera_pitch: 0.0,
            camera_yaw: 0.0,
            min_camera_height: 100.0,
            max_camera_height: 300.0,
            horizontal_scroll: 1.0,
            vertical_scroll: 1.0,
            default_scroll: 0.5,
            rotate_speed: 0.1,
        }
    }
}

impl GameDataIni {
    /// Reads one INI file of `GameData` blocks.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        let mut open = false;
        for line in text.lines() {
            match (open, line.word().as_str()) {
                (false, "gamedata") => open = true,
                (false, "end") => return Err(IniError::StrayEnd { line: line.number }),
                (false, _) => return Err(IniError::NotBlock { line: line.number }),
                (true, "end") => open = false,
                (true, "adjustclifftextures") => {
                    self.adjust_cliff_textures = GameDataIni::yes(&line)?;
                }
                (true, "use3wayterrainblends") => self.three_way_blends = GameDataIni::int(&line)?,
                (true, word) => {
                    let field = match word {
                        "camerapitch" => &mut self.camera_pitch,
                        "camerayaw" => &mut self.camera_yaw,
                        "mincameraheight" => &mut self.min_camera_height,
                        "maxcameraheight" => &mut self.max_camera_height,
                        "horizontalscrollspeedfactor" => &mut self.horizontal_scroll,
                        "verticalscrollspeedfactor" => &mut self.vertical_scroll,
                        "keyboarddefaultscrollspeedfactor" => &mut self.default_scroll,
                        "keyboardcamerarotatespeed" => &mut self.rotate_speed,
                        _ => continue,
                    };
                    *field = GameDataIni::real(&line)?;
                }
            }
        }
        if open {
            Err(IniError::Unclosed)
        } else {
            Ok(())
        }
    }

    /// What the client reads of it.
    pub(crate) const fn game_data(&self) -> GameData {
        GameData {
            adjust_cliff_textures: self.adjust_cliff_textures,
            three_way_blends: self.three_way_blends != 0,
        }
    }

    /// The player's camera, as the game's tactical view moves it at its start, its zoom at
    /// its most: a key or the screen's edge scrolls it `SCROLL_PIXELS` a frame times the
    /// axis's factor and the default scroll factor, each pixel `SCROLL_RESOLUTION` pixels of the
    /// view plane, whose width at depth 1 is `2 · tan(fov / 2)` across `DISPLAY_WIDTH` pixels.
    ///
    /// The pitch is clamped to 0.1°–89.9° and the most height raised to the least, as the view
    /// takes them; none for a camera the client takes no other way.
    pub(crate) fn camera_file(&self) -> Option<CameraFile> {
        let plane = 2.0 * (FIELD_OF_VIEW / 2.0).to_radians().tan() / DISPLAY_WIDTH;
        let speed = |factor: f32| {
            factor * SCROLL_PIXELS * self.default_scroll * SCROLL_RESOLUTION * plane * FRAMES
        };
        CameraFile {
            pitch: self.camera_pitch.clamp(0.1, 89.9),
            yaw: self.camera_yaw,
            field_of_view: FIELD_OF_VIEW,
            height: CameraHeight {
                least: self.min_camera_height,
                most: self.max_camera_height.max(self.min_camera_height),
            },
            scroll: [speed(self.horizontal_scroll), speed(self.vertical_scroll)],
            turn: self.rotate_speed * FRAMES,
            drag_turn: DRAG_TURN,
            zoom_notch: ZOOM_HEIGHT,
            zoom_key: ZOOM_HEIGHT * FRAMES,
        }
        .checked()
    }

    /// The value of a field of yes or no, of either case, as `INI::scanBool` reads it.
    fn yes(line: &IniLine<'_>) -> Result<bool, IniError> {
        let value = GameDataIni::value(line)?;
        if value.eq_ignore_ascii_case("yes") {
            Ok(true)
        } else if value.eq_ignore_ascii_case("no") {
            Ok(false)
        } else {
            Err(IniError::Bool { line: line.number })
        }
    }

    /// The value of a field of a whole number.
    fn int(line: &IniLine<'_>) -> Result<i32, IniError> {
        GameDataIni::value(line)?
            .parse()
            .map_err(|error| IniError::Number {
                line: line.number,
                error,
            })
    }

    /// The value of a field of a real number.
    fn real(line: &IniLine<'_>) -> Result<f32, IniError> {
        GameDataIni::value(line)?
            .parse()
            .map_err(|error| IniError::Real {
                line: line.number,
                error,
            })
    }

    fn value<'a>(line: &IniLine<'a>) -> Result<&'a str, IniError> {
        line.tokens
            .get(1)
            .copied()
            .ok_or(IniError::NoValue { line: line.number })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_keeps_the_default_until_a_file_sets_it_and_the_last_file_wins() {
        let mut ini = GameDataIni::default();
        assert_eq!(
            ini.game_data(),
            GameData {
                adjust_cliff_textures: false,
                three_way_blends: true,
            }
        );
        ini.read(b"GameData\n  MapName = NoName.map\nEnd\n")
            .unwrap();
        ini.read(b"GameData\n  AdjustCliffTextures = Yes\n  Use3WayTerrainBlends = 0\nEnd\n")
            .unwrap();
        assert_eq!(
            ini.game_data(),
            GameData {
                adjust_cliff_textures: true,
                three_way_blends: false,
            }
        );
        ini.read(b"GameData\n  adjustclifftextures = NO\n  Use3WayTerrainBlends = 2\nEnd\n")
            .unwrap();
        assert_eq!(
            ini.game_data(),
            GameData {
                adjust_cliff_textures: false,
                three_way_blends: true,
            }
        );

        // The shipped camera: a key scrolls 100 pixels a frame times 0.5 and the axis's factor,
        // each 250 pixels of a view plane 2 · tan(25°) = 0.9326153 wide across 800: across, 1.6 ·
        // 50 · 250 · 0.9326153 / 800 · 30 = 699.4615 m/s, along, at 2.0, 874.3269 m/s; turning
        // 0.1 rad a frame is 3 rad/s; zooming 10 a frame, 300 a second.
        let mut shipped = GameDataIni::default();
        shipped
            .read(b"GameData\n CameraPitch = 37.5\n MaxCameraHeight = 310.0\n MinCameraHeight = 120.0\n HorizontalScrollSpeedFactor = 1.6;\n VerticalScrollSpeedFactor = 2.0\nEnd\n")
            .unwrap();
        let camera = shipped.camera_file().unwrap();
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
        // The tangent is the platform's, which may differ in its last bit.
        let near = |value: f32, expected: f32| (value - expected).abs() < 1e-3;
        assert!(near(camera.scroll[0], 699.4615), "{:?}", camera.scroll);
        assert!(near(camera.scroll[1], 874.3269), "{:?}", camera.scroll);
        assert!(near(camera.turn, 3.0));
        assert_eq!(
            (camera.drag_turn, camera.zoom_notch, camera.zoom_key),
            (0.01, 10.0, 300.0)
        );

        let fails = |text: &[u8]| GameDataIni::default().read(text).unwrap_err();
        assert!(matches!(
            fails(b"GameData\n CameraPitch = steep\nEnd\n"),
            IniError::Real { line: 2, .. }
        ));
        assert_eq!(fails(b"MapName = A.map\n"), IniError::NotBlock { line: 1 });
        assert_eq!(fails(b"End\n"), IniError::StrayEnd { line: 1 });
        assert_eq!(
            fails(b"GameData\n AdjustCliffTextures = True\nEnd\n"),
            IniError::Bool { line: 2 }
        );
        assert!(matches!(
            fails(b"GameData\n Use3WayTerrainBlends = one\nEnd\n"),
            IniError::Number { line: 2, .. }
        ));
        assert_eq!(
            fails(b"GameData\n AdjustCliffTextures\nEnd\n"),
            IniError::NoValue { line: 2 }
        );
        assert_eq!(fails(b"GameData\n"), IniError::Unclosed);
    }
}
