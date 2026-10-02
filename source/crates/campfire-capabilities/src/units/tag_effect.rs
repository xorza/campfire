use crate::units::block::Block;

/// An effect the mode may give a tag, as the script API reference lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagEffect {
    Blocks(Block),
    Hidden,
    Detects,
    Immune,
}

impl TagEffect {
    pub const ALL: [TagEffect; 9] = [
        TagEffect::Blocks(Block::Move),
        TagEffect::Blocks(Block::Attack),
        TagEffect::Blocks(Block::Cast),
        TagEffect::Blocks(Block::Use),
        TagEffect::Blocks(Block::Target),
        TagEffect::Blocks(Block::Damage),
        TagEffect::Hidden,
        TagEffect::Detects,
        TagEffect::Immune,
    ];

    /// As a mode's `[tags.<name>]` writes it.
    pub fn name(self) -> String {
        match self {
            TagEffect::Blocks(block) => format!("blocks = [\"{}\"]", block.name()),
            TagEffect::Hidden => "hidden = true".to_owned(),
            TagEffect::Detects => "detects = true".to_owned(),
            TagEffect::Immune => "immune = [tags]".to_owned(),
        }
    }
}
