use serde::Deserialize;

/// What a tag can stop its unit doing, or being: moving, the actions of a group, being chosen
/// as a target, taking damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Block {
    Move,
    Attack,
    Cast,
    Use,
    Target,
    Damage,
}

impl Block {
    pub const ALL: [Block; 6] = [
        Block::Move,
        Block::Attack,
        Block::Cast,
        Block::Use,
        Block::Target,
        Block::Damage,
    ];

    /// As a mode's `blocks` writes it.
    pub const fn name(self) -> &'static str {
        match self {
            Block::Move => "move",
            Block::Attack => "attack",
            Block::Cast => "cast",
            Block::Use => "use",
            Block::Target => "target",
            Block::Damage => "damage",
        }
    }
}
