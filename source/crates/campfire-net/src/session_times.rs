use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How long a session waits: a player whose link failed keeps their slot for `grace`, by the
/// server's real clock; a server that stops restores the session within `restore_window` of its
/// journal's last write. The offer gives both, so a client knows how long to try again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTimes {
    pub grace: Duration,
    pub restore_window: Duration,
}

impl SessionTimes {
    /// What a server waits unless its host says: a minute of grace, two of restore window.
    pub const DEFAULT: SessionTimes = SessionTimes {
        grace: Duration::from_secs(60),
        restore_window: Duration::from_secs(120),
    };
}
