use std::fmt;

use campfire_capabilities::{ActionSlots, DeclaredName};

use crate::error::place::Place;

/// What is wrong with the mode's slot kinds or choices, or with a name of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChoiceProblem {
    /// The mode declares more slot kinds than `ActionSlots::LIMIT`.
    TooManySlotKinds,
    /// Data or a script at `at` names a slot kind the mode does not declare.
    UnknownSlotKind { at: Place, kind: String },
    /// A script at `at` names a choice the mode does not declare.
    UnknownChoice { at: Place, name: String },
    /// A choice of loadout entries names no slot kind, or a choice of avatars names one.
    ChoiceSlot(DeclaredName),
    /// Two choices of loadout entries fill slot kinds of other ranks.
    LoadoutRanks,
}

impl fmt::Display for ChoiceProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChoiceProblem::TooManySlotKinds => {
                write!(f, "more than {} slot kinds", ActionSlots::LIMIT)
            }
            ChoiceProblem::UnknownSlotKind { at, kind } => write!(f, "{at}: no slot kind {kind:?}"),
            ChoiceProblem::UnknownChoice { at, name } => write!(f, "{at}: no choice {name:?}"),
            ChoiceProblem::ChoiceSlot(choice) => write!(
                f,
                "choice {choice}: a choice of loadout entries fills a slot kind, one of avatars none"
            ),
            ChoiceProblem::LoadoutRanks => {
                f.write_str("choices of loadout entries fill slot kinds of other ranks")
            }
        }
    }
}
