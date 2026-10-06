use std::num::NonZeroU32;

use serde::Deserialize;

/// The mode's `[saves]`: who may ask for a save, beside the mode, and how often the mode saves on
/// its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SavesData {
    pub by: SaveBy,
    /// Saves at each multiple of this from the session's start, none without it.
    pub autosave_ms: Option<NonZeroU32>,
}

/// Who asks for a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveBy {
    /// The player, on a local server, and the mode.
    #[default]
    Player,
    /// The mode alone, by `ctx.save()` and its autosaves.
    Mode,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_read_with_their_defaults_and_refuse_what_they_do_not_know() {
        let read = |text: &str| toml::from_str::<SavesData>(text);
        assert_eq!(
            read("").unwrap(),
            SavesData {
                by: SaveBy::Player,
                autosave_ms: None,
            }
        );
        assert_eq!(
            read("by = \"mode\"\nautosave_ms = 12000").unwrap(),
            SavesData {
                by: SaveBy::Mode,
                autosave_ms: NonZeroU32::new(12_000),
            }
        );
        assert!(read("by = \"anyone\"").is_err());
        assert!(read("autosave_ms = 0").is_err());
        assert!(read("autosave = 1").is_err());
    }
}
