use campfire_common::NamedEnum;
use serde::{Deserialize, Deserializer};

/// What a tag can stop its unit doing, or being: moving, the actions of a group, producing,
/// being chosen as a target, taking damage. Each member's name is as a mode's `blocks` writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, NamedEnum)]
pub enum Block {
    Move,
    Attack,
    Cast,
    Use,
    Produce,
    Target,
    Damage,
}

impl<'de> Deserialize<'de> for Block {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize_member(deserializer)
    }
}

#[cfg(test)]
mod tests;
