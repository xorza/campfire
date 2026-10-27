use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;
use crate::hud_file::minimap::Minimap;
use crate::hud_file::panel_grid::PanelGrid;

/// A mode's `client/hud.toml`: which units show their bars, how a hit shows, the command panel's
/// grid and the minimap; a mode of no file, or a field the file does not give, shows every unit's
/// bars, a ring for each hit, the default grid and the default minimap (02-engine-core.md, Bars in
/// screen space; 02-engine-core.md, The mode chooses its hit mark; 02-engine-core.md, A look
/// places its buttons).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HudFile {
    pub bars: ShownBars,
    pub hit_mark: HitMark,
    pub panel: PanelGrid,
    pub minimap: Minimap,
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

pub(crate) mod minimap;
pub(crate) mod panel_grid;

#[cfg(test)]
mod tests;
