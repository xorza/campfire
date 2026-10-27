use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops a command set's button from its producer's panel, or a part of one
/// (09-zero-hour.md, The panel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum PanelDropCause {
    /// A button at a place past 14, which the game's control bar does not draw.
    Place,
    /// A button of a command the engine holds no order for, as a general's power or a guard.
    Command,
    /// A button of no image, or of an image no mapped image file gives.
    NoImage,
    /// A button's image stored turned a quarter, which an icon's rectangle does not turn.
    Rotated,
    /// A button's image of a texture no archive holds.
    NoTexture,
    /// A button whose label marks no key, or marks a character no key types.
    NoKey,
}
