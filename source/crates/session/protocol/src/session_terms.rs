use std::num::NonZeroU32;

use campfire_common::{DomainHasher, HashDomain, MapName, PackageFingerprint, Version};
use secp256k1::XOnlyPublicKey;
use serde::{Deserialize, Serialize};

use crate::delegation::Delegation;
use crate::input_rules::InputRules;
use crate::seat_plan::SeatPlan;
use crate::seed_commitment::SeedCommitment;
use crate::session_id::SessionId;

/// Starts the session id, so no other BLAKE3 use can produce one.
const SESSION_ID_DOMAIN: HashDomain<SessionId> = HashDomain::new("campfire/session-id/v5");

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
    /// The limits on its inputs.
    pub inputs: InputRules,
    /// The server's commitment to its seed chain. It is fresh for every session, so no two
    /// sessions share an id.
    pub seed_commitment: SeedCommitment,
    /// The tag of the engine release the session runs on.
    pub release: Version,
    /// The fingerprint of the mode package.
    pub mode: PackageFingerprint,
    /// The mode's map the session plays.
    pub map: MapName,
    /// The fingerprints of the mode's dependencies, in the order of their names in its manifest.
    pub dependencies: Vec<PackageFingerprint>,
    /// How the server opens each seat the session plays, by seat.
    pub seats: Vec<SeatPlan>,
    /// Who receives each kind of the session's state, as the mode and its host set it: the bytes
    /// `capabilities`' `Visibility` encodes, which the protocol carries and does not read.
    pub visibility: Vec<u8>,
    /// The place of the value the host picked of each of the mode's options, among the option's
    /// values, in the order of their names: bytes the protocol carries and does not read
    /// (08-sessions.md, The host picks, and the terms record the pick).
    pub options: Vec<u8>,
    /// The place of the value the host picked of each of the mode's choices of values for each
    /// seat, among the choice's values, 255 for none, seat by seat, each seat's choices in the
    /// order of their names: bytes the protocol carries and does not read (08-sessions.md,
    /// The terms hold each seat's picks).
    pub seat_picks: Vec<u8>,
}

impl SessionTerms {
    /// `BLAKE3(domain ‖ server key ‖ u32 tick rate ‖ u64 max delay ‖ u64 max lead ‖ u32 max
    /// payload length ‖ u32 max inputs per tick ‖ seed commitment ‖ u64 release length ‖ release
    /// ‖ mode fingerprint ‖ u64 map name length ‖ map name ‖ u64 dependency count ‖ dependency
    /// fingerprints ‖ u64 seat count ‖ u8 per seat, its plan's code ‖ u64 visibility length ‖
    /// visibility ‖ u64 option count ‖ u8 per option, its pick's place ‖ u64 seat pick count ‖ u8
    /// per seat pick, its place)`, integers little-endian.
    pub fn session_id(&self) -> SessionId {
        let release = self.release.as_str();
        let mut hasher = DomainHasher::new(SESSION_ID_DOMAIN);
        hasher
            .update(&self.server_key.serialize())
            .update(&self.tick_hz.get().to_le_bytes())
            .update(&self.inputs.max_delay.get().to_le_bytes())
            .update(&self.inputs.max_lead.get().to_le_bytes())
            .update(&self.inputs.max_payload_len.to_le_bytes())
            .update(&self.inputs.max_per_tick.to_le_bytes())
            .update(self.seed_commitment.as_bytes())
            .update(&len(release.len()).to_le_bytes())
            .update(release.as_bytes())
            .update(self.mode.as_bytes())
            .update(&len(self.map.as_str().len()).to_le_bytes())
            .update(self.map.as_str().as_bytes())
            .update(&len(self.dependencies.len()).to_le_bytes());
        for dependency in &self.dependencies {
            hasher.update(dependency.as_bytes());
        }
        hasher.update(&len(self.seats.len()).to_le_bytes());
        for plan in &self.seats {
            hasher.update(&[plan.code()]);
        }
        hasher
            .update(&len(self.visibility.len()).to_le_bytes())
            .update(&self.visibility)
            .update(&len(self.options.len()).to_le_bytes())
            .update(&self.options)
            .update(&len(self.seat_picks.len()).to_le_bytes())
            .update(&self.seat_picks);
        hasher.finish()
    }

    /// The most bytes a journal record of a session of these terms can hold; `None` past a
    /// `u64`. Each record is bounded by them: a packet by its inputs and their payloads; a server
    /// input by a bot's payload or a delegation, whose JSON `Delegation::MAX_JSON` bounds; a
    /// checkpoint by its carry, each seat's delegation and its inputs still to apply, which the
    /// stamps' window bounds, as a seat logs at most `max_per_tick` of a stamp and none stamped
    /// past `max_lead` nor before `max_delay`; the header by the terms and a delegation a seat.
    pub(crate) fn largest_record(&self) -> Option<u64> {
        let payload = u64::from(self.inputs.max_payload_len);
        let inputs = u64::from(self.inputs.max_per_tick);
        let seats = len(self.seats.len());
        let delegation = len(Delegation::MAX_JSON) + VARINT_32;
        let input = VARINT_64 + VARINT_32 + payload;
        let packet = inputs.checked_mul(input)?.checked_add(SIGNATURE + SLACK)?;
        let server = payload.max(delegation) + SIGNATURE + SLACK;
        let window = self
            .inputs
            .max_delay
            .get()
            .checked_add(self.inputs.max_lead.get())?
            .checked_add(1)?;
        // Each carried input holds its tick and its seat beside what its packet held.
        let carried = input + VARINT_64 + VARINT_32;
        let pending = inputs.checked_mul(window)?.checked_mul(carried)?;
        let seat = delegation.checked_add(pending)?.checked_add(SLACK)?;
        let checkpoint = seats
            .checked_mul(seat)?
            .checked_add(2 * HASH + SIGNATURE + SLACK)?;
        let terms = len(self.dependencies.len())
            .checked_mul(HASH)?
            .checked_add(len(self.release.as_str().len()) + len(self.map.as_str().len()) + SLACK)?
            .checked_add(len(self.visibility.len()))?
            .checked_add(len(self.options.len()))?
            .checked_add(len(self.seat_picks.len()))?;
        let header = seats.checked_mul(delegation + SLACK)?.checked_add(terms)?;
        Some(packet.max(server).max(checkpoint).max(header))
    }
}

fn len(len: usize) -> u64 {
    u64::try_from(len).expect("a length fits u64")
}
