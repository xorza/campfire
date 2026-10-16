use serde::de::Error;
use serde::{Deserialize, Deserializer};

campfire_common::enum_all! {
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
    pub const ALL;
}

impl Block {
    /// Each block's name, in the order of `ALL`.
    const NAMES: [&'static str; Self::ALL.len()] = {
        let mut names = [""; Self::ALL.len()];
        let mut at = 0;
        while at < names.len() {
            names[at] = Self::ALL[at].name();
            at += 1;
        }
        names
    };

    /// As a mode's `blocks` writes it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Attack => "attack",
            Self::Cast => "cast",
            Self::Use => "use",
            Self::Target => "target",
            Self::Damage => "damage",
        }
    }
}

/// A block reads as the name `name` gives it, so the two never differ.
impl<'de> Deserialize<'de> for Block {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let found = Self::ALL.into_iter().find(|block| block.name() == text);
        found.ok_or_else(|| D::Error::unknown_variant(&text, &Self::NAMES))
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
