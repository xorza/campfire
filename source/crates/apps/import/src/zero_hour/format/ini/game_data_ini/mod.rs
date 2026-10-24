use std::num::NonZeroU32;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// What the import reads of Zero Hour's `GameData.ini` files, as `GlobalData` reads them: each
/// field from the game's default until a file sets it, a later file's value replacing an earlier
/// one.
#[derive(Debug)]
pub(crate) struct GameDataIni {
    pub(crate) terrain: TerrainDraw,
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
    /// `ValuePerSupplyBox`: the money a supply box pays.
    pub(crate) box_value: NonZeroU32,
}

/// What the terrain's conversion reads of `GameData.ini`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainDraw {
    /// `AdjustCliffTextures`: a steep cell's tile is stretched to its slope, by its cliff UVs or
    /// by its heights.
    pub(crate) adjust_cliff_textures: bool,
    /// `Use3WayTerrainBlends`: a cell draws its third blend.
    pub(crate) three_way_blends: bool,
}

impl Default for GameDataIni {
    /// The game's defaults.
    fn default() -> Self {
        Self {
            terrain: TerrainDraw {
                adjust_cliff_textures: false,
                three_way_blends: true,
            },
            camera_pitch: 0.0,
            camera_yaw: 0.0,
            min_camera_height: 100.0,
            max_camera_height: 300.0,
            horizontal_scroll: 1.0,
            vertical_scroll: 1.0,
            default_scroll: 0.5,
            rotate_speed: 0.1,
            global_lights: 3,
            box_value: NonZeroU32::new(100).expect("100 is no zero"),
        }
    }
}

impl GameDataIni {
    /// The most lights the game draws a map in (`MAX_GLOBAL_LIGHTS`).
    const MOST_LIGHTS: usize = 3;

    /// Reads one INI file of `GameData` blocks.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes)
            .lines()
            .blocks(BlockKinds::One("gamedata"))
        {
            for line in &block?.fields {
                match line.word() {
                    "adjustclifftextures" => self.terrain.adjust_cliff_textures = line.yes()?,
                    // Any count but 0 draws the third blend, as the game tests it.
                    "use3wayterrainblends" => self.terrain.three_way_blends = line.int()? != 0,
                    "numbergloballights" => {
                        // The game keeps three lights a map, and draws past none it keeps.
                        let lights = line.int()?;
                        self.global_lights = usize::try_from(lights)
                            .ok()
                            .filter(|&lights| lights <= Self::MOST_LIGHTS)
                            .ok_or(IniError::Range { line: line.number })?;
                    }
                    // A box of no money, or of less, is a dock that pays nothing, which no known
                    // install ships.
                    "valuepersupplybox" => {
                        let value = u32::try_from(line.int()?).ok().and_then(NonZeroU32::new);
                        self.box_value = value.ok_or(IniError::Range { line: line.number })?;
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
mod tests;
