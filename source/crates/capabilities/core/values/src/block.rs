use campfire_common::NamedEnum;
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

/// As a mode's `blocks` writes each.
impl NamedEnum for Block {
    const ALL: &'static [Self] = &Self::ALL;
    const NAMES: &'static [&'static str] = &["move", "attack", "cast", "use", "target", "damage"];
}

impl<'de> Deserialize<'de> for Block {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize_named(deserializer)
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
