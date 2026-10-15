use std::num::NonZeroU32;

use campfire_script::{DataSection, DataTable};
use serde::{Deserialize, Serialize};

/// The mode's `[saves]`: who may ask for a save, beside the mode, and how often the mode saves on
/// its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SavesData {
    pub by: SaveBy,
    /// Saves at each multiple of this from the session's start, none without it.
    pub autosave_ms: Option<NonZeroU32>,
}

/// Who asks for a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveBy {
    /// The player, on a local server, and the mode.
    #[default]
    Player,
    /// The mode alone, by `ctx.save()` and its autosaves.
    Mode,
}

impl DataSection for SavesData {
    const TABLE: DataTable = DataTable::Saves;
}

#[cfg(test)]
mod tests;
