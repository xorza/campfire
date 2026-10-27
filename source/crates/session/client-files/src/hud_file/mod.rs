use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;

/// A mode's `client/hud.toml`: which units show their bars, and how a hit shows; a mode of no
/// file, or a field the file does not give, shows every unit's bars and a ring for each hit
/// (02-engine-core.md, Bars in screen space; 02-engine-core.md, The mode chooses its hit mark).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HudFile {
    pub bars: ShownBars,
    pub hit_mark: HitMark,
}

/// Which units show their bars: all of them, as a MOBA shows them; or those the player selected
/// and the one under the cursor, as Zero Hour's `Drawable::drawHealthBar` shows them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShownBars {
    #[default]
    All,
    Selected,
}

/// What a unit shows as its life drops: a ring on the ground round it, as a MOBA's HUD marks a
/// hit; or nothing, as Zero Hour marks none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitMark {
    #[default]
    Ring,
    Hidden,
}

impl HudFile {
    /// Where a mode's package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/hud.toml")
    }
}

impl ClientFile for HudFile {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

#[cfg(test)]
mod tests;
