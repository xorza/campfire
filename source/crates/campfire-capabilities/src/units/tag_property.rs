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
    pub const ALL: [TagProperty; 9] = [
        TagProperty::Blocks(Block::Move),
        TagProperty::Blocks(Block::Attack),
        TagProperty::Blocks(Block::Cast),
        TagProperty::Blocks(Block::Use),
        TagProperty::Blocks(Block::Target),
        TagProperty::Blocks(Block::Damage),
        TagProperty::Hidden,
        TagProperty::Detects,
        TagProperty::Immune,
    ];

    /// As a mode's `[tags.<name>]` writes it.
    pub fn name(self) -> String {
        match self {
            TagProperty::Blocks(block) => format!("blocks = [\"{}\"]", block.name()),
            TagProperty::Hidden => "hidden = true".to_owned(),
            TagProperty::Detects => "detects = true".to_owned(),
            TagProperty::Immune => "immune = [tags]".to_owned(),
        }
    }
}
