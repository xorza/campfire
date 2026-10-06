use serde::{Deserialize, Serialize};

/// A player's save command, which a local server takes: a save at the boundary after the next
/// tick, or a load of the latest save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveCommand {
    Save,
    LoadLatest,
}
