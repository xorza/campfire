use std::fmt;

use campfire_common::NamedEnum;

use crate::block::Block;

/// A property the mode may give a tag, as the script API reference lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagProperty {
    Blocks(Block),
    Hidden,
    Detects,
    Immune,
}

/// As a mode's `[tags.<name>]` writes it.
impl fmt::Display for TagProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blocks(block) => write!(f, "blocks = [\"{}\"]", block.name()),
            Self::Hidden => f.write_str("hidden = true"),
            Self::Detects => f.write_str("detects = true"),
            Self::Immune => f.write_str("immune = [tags]"),
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::block::Block;
    use crate::tag_property::TagProperty;

    impl TagProperty {
        /// A block of each of `Block::ALL`, then the rest.
        pub const ALL: [Self; Block::ALL.len() + 3] = {
            let mut all = [Self::Immune; Block::ALL.len() + 3];
            let mut at = 0;
            while at < Block::ALL.len() {
                all[at] = Self::Blocks(Block::ALL[at]);
                at += 1;
            }
            all[at] = Self::Hidden;
            all[at + 1] = Self::Detects;
            all
        };
    }
}
