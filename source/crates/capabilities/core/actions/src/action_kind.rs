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
    /// The kind as one whose range is in meters at every rank, when it is one.
    pub const fn metered(self) -> Option<MeteredKind> {
        match self {
            ActionKind::Use => Some(MeteredKind::Use),
            ActionKind::Build => Some(MeteredKind::Build),
            ActionKind::Gather => Some(MeteredKind::Gather),
            ActionKind::Cast
            | ActionKind::Attack
            | ActionKind::Enter
            | ActionKind::Train
            | ActionKind::Craft => None,
        }
    }
}
