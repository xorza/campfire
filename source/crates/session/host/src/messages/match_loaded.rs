use campfire_common::Tick;
use serde::{Deserialize, Serialize};

/// Tells each joined client that the session went back to a save, before sim tick `tick`, in
/// place, its link kept: the client's match and its join end, and it answers the server's next
/// offer as on connect (08-sessions.md, A load goes back in place).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchLoaded {
    pub tick: Tick,
}
