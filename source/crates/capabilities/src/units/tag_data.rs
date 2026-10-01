use serde::Deserialize;

use crate::units::block::Block;

/// A tag's effects as the mode's `[tags.<name>]` declares them; a tag it does not declare has
/// none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagData {
    #[serde(default)]
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub detects: bool,
    /// The tags whose modifiers it holds without effect.
    #[serde(default)]
    pub immune: Vec<String>,
}
