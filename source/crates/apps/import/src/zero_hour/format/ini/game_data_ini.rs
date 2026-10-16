use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// What the import reads of Zero Hour's `GameData.ini` files, as `GlobalData` reads them: each
/// field from the game's default until a file sets it, a later file's value replacing an earlier
/// one.
#[derive(Debug)]
pub(crate) struct GameDataIni {
    pub(crate) adjust_cliff_textures: bool,
    pub(crate) three_way_blends: i32,
    /// `CameraPitch` and `CameraYaw`, in degrees.
    pub(crate) camera_pitch: f32,
    pub(crate) camera_yaw: f32,
    pub(crate) min_camera_height: f32,
    pub(crate) max_camera_height: f32,
    pub(crate) horizontal_scroll: f32,
    pub(crate) vertical_scroll: f32,
    /// `KeyboardDefaultScrollSpeedFactor`: the player's scroll factor until their options set one.
    pub(crate) default_scroll: f32,
    /// `KeyboardCameraRotateSpeed`: radians a frame of the game's 30.
    pub(crate) rotate_speed: f32,
    /// `NumberGlobalLights`: the lights the terrain and the objects are drawn in, of the three
    /// each map gives.
    global_lights: usize,
}

impl Default for GameDataIni {
    /// The game's defaults.
    fn default() -> Self {
        Self {
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
            global_lights: 3,
        }
    }
}

impl GameDataIni {
    /// The most lights the game draws a map in (`MAX_GLOBAL_LIGHTS`).
    const MOST_LIGHTS: usize = 3;

    /// Reads one INI file of `GameData` blocks.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes).blocks(BlockKinds::One("gamedata")) {
            for line in &block?.fields {
                match line.word().as_str() {
                    "adjustclifftextures" => self.adjust_cliff_textures = line.yes()?,
                    "use3wayterrainblends" => self.three_way_blends = line.int()?,
                    "numbergloballights" => {
                        // The game keeps three lights a map, and draws past none it keeps.
                        let lights = line.int()?;
                        self.global_lights = usize::try_from(lights)
                            .ok()
                            .filter(|&lights| lights <= Self::MOST_LIGHTS)
                            .ok_or(IniError::Range { line: line.number })?;
                    }
                    word => {
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
                        *field = line.real()?;
                    }
                }
            }
        }
        Ok(())
    }

    /// How many of each map's object lights the terrain and the objects are drawn in.
    pub(crate) const fn global_lights(&self) -> usize {
        self.global_lights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_keeps_the_default_until_a_file_sets_it_and_the_last_file_wins() {
        // The terrain's fields: `AdjustCliffTextures` off and three-way blends on by default.
        let mut ini = GameDataIni::default();
        let fields = |ini: &GameDataIni| (ini.adjust_cliff_textures, ini.three_way_blends);
        assert_eq!(fields(&ini), (false, 1));
        ini.read(b"GameData\n  MapName = NoName.map\nEnd\n")
            .unwrap();
        ini.read(b"GameData\n  AdjustCliffTextures = Yes\n  Use3WayTerrainBlends = 0\nEnd\n")
            .unwrap();
        assert_eq!(fields(&ini), (true, 0));
        ini.read(b"GameData\n  adjustclifftextures = NO\n  Use3WayTerrainBlends = 2\nEnd\n")
            .unwrap();
        assert_eq!(fields(&ini), (false, 2));

        // The shipped camera's fields, which the camera's conversion reads.
        let mut shipped = GameDataIni::default();
        shipped
            .read(b"GameData\n CameraPitch = 37.5\n MaxCameraHeight = 310.0\n MinCameraHeight = 120.0\n HorizontalScrollSpeedFactor = 1.6;\n VerticalScrollSpeedFactor = 2.0\nEnd\n")
            .unwrap();
        assert_eq!(
            [
                shipped.camera_pitch,
                shipped.min_camera_height,
                shipped.max_camera_height,
                shipped.horizontal_scroll,
                shipped.vertical_scroll,
            ],
            [37.5, 120.0, 310.0, 1.6, 2.0]
        );

        let fails = |text: &[u8]| GameDataIni::default().read(text).unwrap_err();
        assert!(matches!(
            fails(b"GameData\n CameraPitch = steep\nEnd\n"),
            IniError::Real { line: 2, .. }
        ));
        // Three lights by default; a file may draw fewer, and none past the three a map keeps.
        assert_eq!(GameDataIni::default().global_lights(), 3);
        let mut two = GameDataIni::default();
        two.read(b"GameData\n NumberGlobalLights = 2\nEnd\n")
            .unwrap();
        assert_eq!(two.global_lights(), 2);
        assert_eq!(
            fails(b"GameData\n NumberGlobalLights = 4\nEnd\n"),
            IniError::Range { line: 2 }
        );
        assert_eq!(
            fails(b"GameData\n NumberGlobalLights = -1\nEnd\n"),
            IniError::Range { line: 2 }
        );
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
