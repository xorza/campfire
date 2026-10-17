use campfire_values::Block;
use serde::{Deserialize, Serialize};

/// What is special about an action, beside the pipeline every action runs. The release runs every
/// kind but `enter` and `craft`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    #[default]
    Cast,
    Attack,
    Use,
    Enter,
    Train,
    Build,
    Gather,
    Craft,
}

/// A kind of action whose range is in meters at every rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeteredKind {
    Use,
    Build,
    Gather,
}

impl ActionKind {
    /// The kinds the release runs, in the order the table of action fields gives their uses.
    pub const RUN: [Self; 6] = [
        Self::Cast,
        Self::Attack,
        Self::Use,
        Self::Train,
        Self::Build,
        Self::Gather,
    ];

    /// The block that keeps a unit from starting an action of the kind; a slot of a unit's use
    /// group starts as a use whatever its kind (`UseGroup::group`).
    pub const fn block(self) -> Block {
        match self {
            Self::Cast | Self::Craft => Block::Cast,
            Self::Attack => Block::Attack,
            Self::Use | Self::Enter | Self::Build | Self::Gather => Block::Use,
            Self::Train => Block::Produce,
        }
    }

    /// Whether the release runs actions of the kind, one of `RUN`: a planned one, as `enter`, it
    /// does not.
    pub const fn runs(self) -> bool {
        let mut at = 0;
        while at < Self::RUN.len() {
            if Self::RUN[at] as u8 == self as u8 {
                return true;
            }
            at += 1;
        }
        false
    }

    /// The kind as one whose range is in meters at every rank, when it is one.
    pub const fn metered(self) -> Option<MeteredKind> {
        match self {
            Self::Use => Some(MeteredKind::Use),
            Self::Build => Some(MeteredKind::Build),
            Self::Gather => Some(MeteredKind::Gather),
            Self::Cast | Self::Attack | Self::Enter | Self::Train | Self::Craft => None,
        }
    }
}
