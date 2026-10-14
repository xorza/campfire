use secp256k1::Keypair;

use crate::os::Os;
use crate::session_times::SessionTimes;

/// What a server opens or restores a session with, beyond the session itself: its key, which
/// signs what it logs, how long it waits, and its clock and randomness.
#[derive(Debug, Clone)]
pub struct ServerSetup {
    pub key: Keypair,
    pub times: SessionTimes,
    pub os: Os,
}
