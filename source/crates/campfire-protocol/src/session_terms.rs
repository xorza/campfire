use std::num::NonZeroU32;

use blake3::Hasher;
use campfire_common::{Fingerprint, Ticks};
use secp256k1::XOnlyPublicKey;
use serde::{Deserialize, Serialize};

use crate::delegation::Delegation;
use crate::seed_commitment::SeedCommitment;
use crate::session_id::SessionId;
use crate::slot_plan::SlotPlan;

/// Starts the session id, so no other BLAKE3 use can produce one.
const SESSION_ID_DOMAIN: &[u8] = b"campfire/session-id/v1";

/// The most bytes postcard writes for a `u64`, and for a `u32` or a length.
const VARINT_64: u64 = 10;
const VARINT_32: u64 = 5;
/// The bytes of a signature, and of a hash.
const SIGNATURE: u64 = 64;
const HASH: u64 = 32;
/// More than a record's tags, small fields and enum tags take, beside what the bound counts.
const SLACK: u64 = 256;

/// What the server fixes when it opens a session, before any player joins. The session id is
/// their hash, and every delegation and chain-head signature names the id: the players sign
/// these terms, so a log cannot change them. They encode in postcard field by field, in the log's
/// header and in the message that offers them to a joining player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTerms {
    pub server_key: XOnlyPublicKey,
    /// Ticks a second, fixed for the whole session.
    pub tick_hz: NonZeroU32,
    /// The most ticks an input may land after its stamp; a later one is logged as late.
    pub max_input_delay: Ticks,
    /// The most ticks an input's stamp may be ahead of the next tick; a further one is logged as
    /// early. The server holds each input until its tick, so this bounds what a client can make
    /// it hold.
    pub max_input_lead: Ticks,
    /// The most bytes an input's payload may hold.
    pub max_payload_len: u32,
    /// The most inputs of one stamp, and of one packet, a player may send, and the most of its
    /// inputs that apply in one tick, the rest waiting for the next. With the max payload length
    /// and the max input lead, it bounds how fast a player can grow the log.
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
    /// How the server opens each slot the session plays, by slot.
    pub slots: Vec<SlotPlan>,
}

impl SessionTerms {
    /// `BLAKE3(domain ‖ server key ‖ u32 tick rate ‖ u64 max delay ‖ u64 max lead ‖ u32 max
    /// payload length ‖ u32 max inputs per tick ‖ seed commitment ‖ u64 release length ‖ release
    /// ‖ mode fingerprint ‖ u64 dependency count ‖ dependency fingerprints ‖ u64 slot count ‖ u8
    /// per slot, its plan's code)`, integers little-endian.
    pub fn session_id(&self) -> SessionId {
        let mut hasher = Hasher::new();
        hasher
            .update(SESSION_ID_DOMAIN)
            .update(&self.server_key.serialize())
            .update(&self.tick_hz.get().to_le_bytes())
            .update(&self.max_input_delay.get().to_le_bytes())
            .update(&self.max_input_lead.get().to_le_bytes())
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
        hasher.update(&len(self.slots.len()).to_le_bytes());
        for plan in &self.slots {
            hasher.update(&[plan.code()]);
        }
        SessionId::new(*hasher.finalize().as_bytes())
    }

    /// The most bytes a journal record of a session of these terms can hold; `None` past a
    /// `u64`. Each record is bounded by them: a packet by its inputs and their payloads; a server
    /// input by a bot's payload or a delegation, whose JSON `Delegation::MAX_JSON` bounds; a
    /// checkpoint by its carry, each slot's delegation and its inputs still to apply, which the
    /// stamps' window bounds, as a slot logs at most `max_inputs_per_tick` of a stamp and none
    /// stamped past `max_input_lead` nor before `max_input_delay`; the header by the terms and
    /// a delegation a slot.
    pub(crate) fn largest_record(&self) -> Option<u64> {
        let payload = u64::from(self.max_payload_len);
        let inputs = u64::from(self.max_inputs_per_tick);
        let slots = len(self.slots.len());
        let delegation = len(Delegation::MAX_JSON) + VARINT_32;
        let input = VARINT_64 + VARINT_32 + payload;
        let packet = inputs.checked_mul(input)?.checked_add(SIGNATURE + SLACK)?;
        let server = payload.max(delegation) + SIGNATURE + SLACK;
        let window = self
            .max_input_delay
            .get()
            .checked_add(self.max_input_lead.get())?
            .checked_add(1)?;
        // Each carried input holds its tick and its slot beside what its packet held.
        let carried = input + VARINT_64 + VARINT_32;
        let pending = inputs.checked_mul(window)?.checked_mul(carried)?;
        let slot = delegation.checked_add(pending)?.checked_add(SLACK)?;
        let checkpoint = slots
            .checked_mul(slot)?
            .checked_add(2 * HASH + SIGNATURE + SLACK)?;
        let terms = len(self.dependencies.len())
            .checked_mul(HASH)?
            .checked_add(len(self.release.len()) + SLACK)?;
        let header = slots.checked_mul(delegation + SLACK)?.checked_add(terms)?;
        Some(packet.max(server).max(checkpoint).max(header))
    }
}

fn len(len: usize) -> u64 {
    u64::try_from(len).expect("a length fits u64")
}
