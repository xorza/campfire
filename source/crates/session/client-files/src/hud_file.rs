use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::client_path;

/// A mode's `client/hud.toml`: which units show their bars; a mode of no file, or a field the
/// file does not give, shows every unit's (02-engine-core.md, Bars in screen space).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HudFile {
    pub bars: ShownBars,
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

impl HudFile {
    /// Where a mode's package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/hud.toml")
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_hud_shows_every_unit_s_bars_unless_its_file_says_the_selected() {
        let selected = HudFile {
            bars: ShownBars::Selected,
        };
        assert_eq!(
            Toml::parse::<HudFile>("bars = \"selected\"").unwrap(),
            selected
        );
        assert_eq!(Toml::parse::<HudFile>("").unwrap().bars, ShownBars::All);
        assert_eq!(
            Toml::parse::<HudFile>(&Toml::write(&selected)).unwrap(),
            selected
        );
        assert!(Toml::parse::<HudFile>("bars = \"some\"").is_err());
        assert!(Toml::parse::<HudFile>("rings = \"all\"").is_err());
    }
}
