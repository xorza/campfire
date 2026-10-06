use campfire_protocol::CertificateHash;
use campfire_protocol::secp256k1::Keypair;

use crate::session_times::SessionTimes;

/// What a server opens or restores a session with, beyond the session itself: its key, which
/// signs what it logs, the hash of the TLS certificate its transport presents, how long it
/// waits, and its clock and randomness.
#[derive(Debug, Clone, Copy)]
pub struct ServerSetup {
    pub key: Keypair,
    pub certificate: CertificateHash,
    pub times: SessionTimes,
    /// Unix seconds, against which a delegation's expiry is checked.
    pub clock: fn() -> u64,
    /// Fills a challenge or BIP-340's auxiliary randomness with random bytes.
    pub entropy: fn(&mut [u8; 32]),
}
