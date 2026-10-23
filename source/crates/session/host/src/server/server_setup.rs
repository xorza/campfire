use std::sync::Arc;

use campfire_protocol::Signer;

use crate::os::Os;
use crate::session_times::SessionTimes;

/// What a server opens or restores a session with, beyond the session itself: the signer of its
/// key, which signs what it logs, how long it waits, and its clock and randomness.
#[derive(Debug, Clone)]
pub struct ServerSetup {
    pub signer: Arc<Signer>,
    pub times: SessionTimes,
    pub os: Os,
}
