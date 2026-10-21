use campfire_values::{FilterData, PackagePath};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;

/// A mode's `client/picking.toml`: the tiers that rank the units under the pointer, a unit in
/// the tier of the first filter that selects it and one no filter selects in a last tier, and
/// the least radius on the screen a unit's shape widens to; a mode of no file, or a field the
/// file does not give, has one tier and no least size (02-engine-core.md, The mode chooses its
/// picking).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PickingFile {
    pub tiers: Vec<FilterData>,
    pub least_radius_px: PixelRadius,
}

/// A radius on the screen in logical pixels: finite and at least 0.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PixelRadius(f32);

impl PickingFile {
    /// Where a mode's package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/picking.toml")
    }
}

impl ClientFile for PickingFile {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

impl PixelRadius {
    /// The radius of `pixels`; none unless it is finite and at least 0.
    pub const fn new(pixels: f32) -> Option<Self> {
        if pixels.is_finite() && pixels >= 0.0 {
            Some(Self(pixels))
        } else {
            None
        }
    }

    pub const fn get(self) -> f32 {
        self.0
    }
}

/// A finite radius is equal to itself.
impl Eq for PixelRadius {}

/// A package's file is untrusted, so a radius not finite, or negative, fails to read.
impl<'de> Deserialize<'de> for PixelRadius {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let pixels = f32::deserialize(deserializer)?;
        Self::new(pixels).ok_or_else(|| D::Error::custom("a radius is finite and at least 0"))
    }
}

impl Serialize for PixelRadius {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[cfg(test)]
mod tests;
