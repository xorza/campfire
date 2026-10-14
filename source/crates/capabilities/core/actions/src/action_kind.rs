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
