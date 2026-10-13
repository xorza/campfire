use std::fmt;

use crate::units::block::Block;

/// A property the mode may give a tag, as the script API reference lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagProperty {
    Blocks(Block),
    Hidden,
    Detects,
    Immune,
}

impl TagProperty {
    /// A block of each of `Block::ALL`, then the rest.
    pub const ALL: [TagProperty; Block::ALL.len() + 3] = {
        let mut all = [TagProperty::Immune; Block::ALL.len() + 3];
        let mut at = 0;
        while at < Block::ALL.len() {
            all[at] = TagProperty::Blocks(Block::ALL[at]);
            at += 1;
        }
        all[at] = TagProperty::Hidden;
        all[at + 1] = TagProperty::Detects;
        all
    };
}

/// As a mode's `[tags.<name>]` writes it.
impl fmt::Display for TagProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TagProperty::Blocks(block) => write!(f, "blocks = [\"{}\"]", block.name()),
            TagProperty::Hidden => f.write_str("hidden = true"),
            TagProperty::Detects => f.write_str("detects = true"),
            TagProperty::Immune => f.write_str("immune = [tags]"),
        }
    }
}
