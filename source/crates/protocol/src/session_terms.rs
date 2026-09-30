use std::num::NonZeroU32;

use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::fingerprint::Fingerprint;
use crate::server_seed::SeedCommitment;
use crate::session_id::SessionId;

/// Starts the session id, so no other BLAKE3 use can produce one.
const SESSION_ID_DOMAIN: &[u8] = b"campfire/session-id/v1";

/// What the server fixes when it opens a session, before any player joins. The session id is
/// their hash, and every delegation and chain-head signature names the id: the players sign
/// these terms, so a log cannot change them. They encode in postcard field by field, in the log's
/// header and in the message that offers them to a joining player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTerms {
    /// The server's x-only public key.
    pub server_key: [u8; 32],
    /// Ticks a second, fixed for the whole session.
    pub tick_hz: NonZeroU32,
    /// The most ticks an input may land after its stamp; a later one is logged as late.
    pub max_input_delay: u64,
    /// The most ticks an input's stamp may be ahead of the next tick; a further one is logged as
    /// early. The server holds each input until its tick, so this bounds what a client can make
    /// it hold.
    pub max_input_lead: u64,
    /// The most bytes an input's payload may hold.
    pub max_payload_len: u32,
    /// The most inputs a player may send before one tick. With the max payload length, it bounds
    /// how fast a player can grow the log.
    pub max_inputs_per_tick: u32,
    /// The server's commitment to its seed chain. It is fresh for every session, so no two
    /// sessions share an id.
    pub seed_commitment: SeedCommitment,
    /// The tag of the engine release the session runs on.
    pub release: String,
    /// The fingerprint of the mode package.
    pub mode: Fingerprint,
    /// The fingerprints of the mode's dependencies, in the order of their names in its manifest.
    pub dependencies: Vec<Fingerprint>,
}

impl SessionTerms {
    /// `BLAKE3(domain ‖ server key ‖ u32 tick rate ‖ u64 max delay ‖ u64 max lead ‖ u32 max
    /// payload length ‖ u32 max inputs per tick ‖ seed commitment ‖ u64 release length ‖ release
    /// ‖ mode fingerprint ‖ u64 dependency count ‖ dependency fingerprints)`, integers
    /// little-endian.
    pub fn session_id(&self) -> SessionId {
        let mut hasher = Hasher::new();
        hasher
            .update(SESSION_ID_DOMAIN)
            .update(&self.server_key)
            .update(&self.tick_hz.get().to_le_bytes())
            .update(&self.max_input_delay.to_le_bytes())
            .update(&self.max_input_lead.to_le_bytes())
            .update(&self.max_payload_len.to_le_bytes())
            .update(&self.max_inputs_per_tick.to_le_bytes())
            .update(self.seed_commitment.as_bytes())
            .update(&len(self.release.len()).to_le_bytes())
            .update(self.release.as_bytes())
            .update(self.mode.as_bytes())
            .update(&len(self.dependencies.len()).to_le_bytes());
        for dependency in &self.dependencies {
            hasher.update(dependency.as_bytes());
        }
        SessionId::new(*hasher.finalize().as_bytes())
    }
}

fn len(len: usize) -> u64 {
    u64::try_from(len).expect("a length fits u64")
}
