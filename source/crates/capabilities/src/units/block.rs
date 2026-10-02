use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// What a tag can stop its unit doing, or being: moving, the actions of a group, being chosen
/// as a target, taking damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// Each block's name, in the order of `ALL`.
    const NAMES: [&'static str; Block::ALL.len()] = {
        let mut names = [""; Block::ALL.len()];
        let mut at = 0;
        while at < names.len() {
            names[at] = Block::ALL[at].name();
            at += 1;
        }
        names
    };

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

/// A block reads as the name `name` gives it, so the two never differ.
impl<'de> Deserialize<'de> for Block {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Block, D::Error> {
        let text = String::deserialize(deserializer)?;
        let found = Block::ALL.into_iter().find(|block| block.name() == text);
        found.ok_or_else(|| D::Error::unknown_variant(&text, &Block::NAMES))
    }
}

#[cfg(test)]
mod tests {
    use serde::de::value::{Error as ValueError, StrDeserializer};

    use super::*;

    #[test]
    fn a_block_reads_from_its_name_and_from_no_other() {
        for block in Block::ALL {
            let name = StrDeserializer::<ValueError>::new(block.name());
            assert_eq!(Block::deserialize(name), Ok(block));
        }
        let other = StrDeserializer::<ValueError>::new("Move");
        assert!(Block::deserialize(other).is_err());
    }
}
