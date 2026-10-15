use campfire_values::DeclaredName;
use thiserror::Error;

/// What is wrong with the mode's choices.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ChoiceProblem {
    /// A choice of loadout entries names no slot kind, or a choice of avatars names one.
    #[error("choice {0}: a choice of loadout entries fills a slot kind, one of avatars none")]
    ChoiceSlot(DeclaredName),
    /// Two choices of loadout entries fill slot kinds of other ranks.
    #[error("choices of loadout entries fill slot kinds of other ranks")]
    LoadoutRanks,
}
