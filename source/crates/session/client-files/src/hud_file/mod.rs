use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;
use crate::hud_file::minimap::Minimap;
use crate::hud_file::panel_grid::PanelGrid;
use crate::panel_icon::PanelIcon;

/// A mode's `client/hud.toml`: which units show their bars, how a hit shows, the command panel's
/// grid, the minimap, and the icon each level from 2 shows beside its unit's bars; a mode of no
/// file, or a field the file does not give, shows every unit's bars, a ring for each hit, the
/// default grid, the default minimap and no level (02-engine-core.md, Bars in screen space;
/// 02-engine-core.md, The mode chooses its hit mark; 02-engine-core.md, A look places its buttons;
/// 02-engine-core.md, A level shows its icon).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HudFile {
    pub bars: ShownBars,
    pub hit_mark: HitMark,
    pub panel: PanelGrid,
    pub minimap: Minimap,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub level_icons: Vec<PanelIcon>,
}

impl HudFile {
    /// The icon a unit at `level` shows; none at level 1 and past the last icon.
    pub fn level_icon(&self, level: u32) -> Option<&PanelIcon> {
        let at = usize::try_from(level).ok()?.checked_sub(2)?;
        self.level_icons.get(at)
    }
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
