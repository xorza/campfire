use blake3::Hasher;
use campfire_common::{PlayerSlot, SegmentSeed};

use crate::delegation::Delegation;
use crate::server_seed::ServerSeed;
use crate::session_log::error::SeedError;
use crate::session_terms::SessionTerms;
use crate::slot_start::SlotStart;

/// Starts the segment seed, so no other BLAKE3 use can produce one.
const SEGMENT_SEED_DOMAIN: &[u8] = b"campfire/segment-seed/v1";

/// What the log fixes before the first tick: the session's terms, and how each of its slots
/// starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHeader {
    pub terms: SessionTerms,
    /// Each slot's start, by slot: a player's delegation, which lets their session key sign
    /// their chain heads, whose id their first input links to, and which carries their seed
    /// contribution; a bot; or open.
    pub slots: Vec<SlotStart>,
}

impl SessionHeader {
    /// Segment `segment`'s seed, `BLAKE3(domain ‖ u32 segment ‖ server seed ‖ contributions of
    /// the players who started, in slot order)`; an error when `server_seed` is not that
    /// segment's seed of the committed chain.
    pub fn segment_seed(
        &self,
        segment: u32,
        server_seed: &ServerSeed,
    ) -> Result<SegmentSeed, SeedError> {
        if !server_seed.check(segment, &self.terms.seed_commitment) {
            return Err(SeedError::WrongSeed);
        }
        let mut hasher = Hasher::new();
        hasher
            .update(SEGMENT_SEED_DOMAIN)
            .update(&segment.to_le_bytes())
            .update(server_seed.as_bytes());
        for (_, delegation) in self.players() {
            hasher.update(&delegation.terms().seed_contribution);
        }
        Ok(SegmentSeed::new(*hasher.finalize().as_bytes()))
    }

    /// The players who started, by slot, with their delegations.
    pub fn players(&self) -> impl Iterator<Item = (PlayerSlot, &Delegation)> {
        (0..)
            .zip(&self.slots)
            .filter_map(|(slot, start)| match start {
                SlotStart::Player(delegation) => Some((PlayerSlot::new(slot), &**delegation)),
                SlotStart::Bot | SlotStart::Open => None,
            })
    }
}
