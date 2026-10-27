use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;
use crate::error::FogFileError;

/// A mode's `client/fog.toml`: how much of its light a cell keeps that its player's team sees,
/// explored and never saw, and the fog's color, which each multiplies, both in the sRGB encoding a
/// game multiplies in, as Zero Hour's `W3DShroud` writes `ShroudColor` times its level
/// (02-engine-core.md, The fog is the one material's). A mode of no file keeps all of a seen cell's
/// light, half of an explored one's, and none of a shrouded one's, in white.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FogFile {
    pub seen: FogLevel,
    pub explored: FogLevel,
    pub shrouded: FogLevel,
    pub color: [u8; 3],
}

/// A share of a cell's light, from 0, none, to 255, all of it, as a game's byte holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FogLevel(pub u8);

impl FogFile {
    /// The fog of these levels and `color`; an error for a seen cell darker than an explored one,
    /// or an explored one darker than a shrouded one.
    pub fn new(
        [seen, explored, shrouded]: [FogLevel; 3],
        color: [u8; 3],
    ) -> Result<Self, FogFileError> {
        if !(shrouded <= explored && explored <= seen) {
            return Err(FogFileError::Order);
        }
        Ok(Self {
            seen,
            explored,
            shrouded,
            color,
        })
    }

    /// The fog's light at `level`, of each channel of its color, in the sRGB encoding.
    pub fn light(&self, level: FogLevel) -> [u8; 3] {
        // Each product of two bytes over 255 rounds to the nearest byte, as a byte holds it.
        self.color.map(|channel| {
            let product = u32::from(channel) * u32::from(level.0);
            u8::try_from((product + 127) / 255)
                .expect("a product of two bytes over 255 fits a byte")
        })
    }

    pub fn path() -> PackagePath {
        client_path::parse("client/fog.toml")
    }
}

impl Default for FogFile {
    fn default() -> Self {
        Self {
            seen: FogLevel(u8::MAX),
            explored: FogLevel(128),
            shrouded: FogLevel(0),
            color: [u8::MAX; 3],
        }
    }
}

impl ClientFile for FogFile {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

/// A file whose seen cell is darker than an explored one, or an explored one darker than a
/// shrouded one, fails to read.
impl<'de> Deserialize<'de> for FogFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            seen: FogLevel,
            explored: FogLevel,
            shrouded: FogLevel,
            color: [u8; 3],
        }
        let Fields {
            seen,
            explored,
            shrouded,
            color,
        } = Fields::deserialize(deserializer)?;
        Self::new([seen, explored, shrouded], color).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests;
