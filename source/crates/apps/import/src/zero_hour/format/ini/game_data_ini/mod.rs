use std::num::NonZeroU32;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};
use crate::zero_hour::format::ini::weapon_bonus::{
    DAMAGE, GARRISONED, RANGE, RATE_OF_FIRE, VETERAN, WeaponBonusLine,
};

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
    /// `UnitDamagedThreshold` and `UnitReallyDamagedThreshold`: the shares of its most life at or
    /// below which an object is damaged and really damaged (`calcDamageState`).
    pub(crate) damaged_threshold: f32,
    pub(crate) really_damaged_threshold: f32,
    /// `MaxParticleCount`: the most particles alive at once.
    pub(crate) max_particles: u32,
    /// `Gravity`: the acceleration a falling object takes, in world units a second squared, as
    /// `parseAccelerationReal` reads it; the game's default is −1 a frame squared.
    pub(crate) gravity: f32,
    /// `GroundStiffness`: the share of a falling object's speed down the ground takes as it
    /// lands (`PhysicsBehavior::handleBounce`).
    pub(crate) ground_stiffness: f32,
    /// `MakeTrackMarks`: whether a tread lays a track.
    pub(crate) track_marks: bool,
    /// `MaxTerrainTracks`: the most tracks laid at once.
    pub(crate) most_tracks: u16,
    /// `SellPercentage`, as `parsePercentToReal` reads it, a fraction of 1: the share of its cost
    /// a sold object of no `RefundValue` gives back.
    pub(crate) sell_share: f32,
    /// `ShroudColor`, and the levels of a clear, a fogged and a shrouded cell, `ClearAlpha`,
    /// `FogAlpha` and `ShroudAlpha`, each a byte (`W3DShroud::setShroudLevel`).
    pub(crate) shroud_color: [u8; 3],
    pub(crate) shroud_levels: [u8; 3],
    /// The bonuses of a veteran, an elite and a heroic object, of the global `WeaponBonus`
    /// lines and `HealthBonus_Veteran`, `_Elite` and `_Heroic`, each 1 by default, as the
    /// game's `WeaponBonusSet` and `GlobalData` start them.
    pub(crate) veterancy: [VeterancyBonus; 3],
    /// The bonus of a garrisoned object, of the global `WeaponBonus` lines of `GARRISONED`.
    pub(crate) garrison: GarrisonBonus,
}

/// What a garrison multiplies its riders' weapons' range and damage by, each 1 by default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GarrisonBonus {
    pub(crate) range: f32,
    pub(crate) damage: f32,
}

/// What a level of veterancy multiplies: its weapons' damage and rate of fire, and its most
/// health.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VeterancyBonus {
    pub(crate) damage: f32,
    pub(crate) rate_of_fire: f32,
    pub(crate) health: f32,
}

impl VeterancyBonus {
    /// No bonus.
    const NONE: Self = Self {
        damage: 1.0,
        rate_of_fire: 1.0,
        health: 1.0,
    };
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
            damaged_threshold: 0.5,
            really_damaged_threshold: 0.1,
            max_particles: 0,
            gravity: -(30.0 * 30.0),
            ground_stiffness: 0.5,
            track_marks: false,
            most_tracks: 0,
            sell_share: 1.0,
            shroud_color: [u8::MAX; 3],
            shroud_levels: [255, 127, 0],
            veterancy: [VeterancyBonus::NONE; 3],
            garrison: GarrisonBonus {
                range: 1.0,
                damage: 1.0,
            },
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
                    "maketrackmarks" => self.track_marks = line.yes()?,
                    "maxterraintracks" => {
                        let count = u16::try_from(line.int()?).ok();
                        self.most_tracks = count.ok_or(IniError::Range { line: line.number })?;
                    }
                    "maxparticlecount" => {
                        let count = u32::try_from(line.int()?).ok();
                        self.max_particles = count.ok_or(IniError::Range { line: line.number })?;
                    }
                    "sellpercentage" => self.sell_share = line.real()? / 100.0,
                    "shroudcolor" => {
                        let channels = line.int_coord(["R", "G", "B"])?;
                        for (channel, value) in self.shroud_color.iter_mut().zip(channels) {
                            *channel = u8::try_from(value)
                                .map_err(|_| IniError::Range { line: line.number })?;
                        }
                    }
                    "weaponbonus" => {
                        let bonus = WeaponBonusLine::read(line)?;
                        let level = bonus
                            .condition
                            .checked_sub(VETERAN)
                            .filter(|&level| level < 3);
                        if let Some(level) = level {
                            let veterancy = &mut self.veterancy[level];
                            match bonus.field {
                                DAMAGE => veterancy.damage = bonus.value,
                                RATE_OF_FIRE => veterancy.rate_of_fire = bonus.value,
                                _ => {}
                            }
                        }
                        if bonus.condition == GARRISONED {
                            match bonus.field {
                                DAMAGE => self.garrison.damage = bonus.value,
                                RANGE => self.garrison.range = bonus.value,
                                _ => {}
                            }
                        }
                    }
                    health @ ("healthbonus_veteran"
                    | "healthbonus_elite"
                    | "healthbonus_heroic") => {
                        let at = [
                            "healthbonus_veteran",
                            "healthbonus_elite",
                            "healthbonus_heroic",
                        ]
                        .iter()
                        .position(|&word| word == health)
                        .expect("one of the levels");
                        // `INI::parsePercentToReal`.
                        self.veterancy[at].health = line.real()? / 100.0;
                    }
                    level @ ("clearalpha" | "fogalpha" | "shroudalpha") => {
                        let at = ["clearalpha", "fogalpha", "shroudalpha"]
                            .iter()
                            .position(|&word| word == level)
                            .expect("one of the levels");
                        self.shroud_levels[at] = u8::try_from(line.int()?)
                            .map_err(|_| IniError::Range { line: line.number })?;
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
                            "unitdamagedthreshold" => &mut self.damaged_threshold,
                            "unitreallydamagedthreshold" => &mut self.really_damaged_threshold,
                            "gravity" => &mut self.gravity,
                            "groundstiffness" => &mut self.ground_stiffness,
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
