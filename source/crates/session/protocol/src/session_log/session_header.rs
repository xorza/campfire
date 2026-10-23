use campfire_common::{DomainHasher, HashDomain, PlayerSlot, SegmentSeed};

use crate::delegation::Delegation;
use crate::player_slot_start::PlayerSlotStart;
use crate::server_seed::ServerSeed;
use crate::session_terms::SessionTerms;

/// Starts the segment seed, so no other BLAKE3 use can produce one.
const SEGMENT_SEED_DOMAIN: HashDomain<SegmentSeed> = HashDomain::new("campfire/segment-seed/v1");

/// What the log fixes before the first tick: the session's terms, and how each of its player_slots
/// starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHeader {
    pub terms: SessionTerms,
    /// Each player slot's start, by player slot: a player's delegation, which lets their session
    /// key sign their chain heads, whose id their first input links to, and which carries their
    /// seed contribution; a bot; or open.
    pub player_slots: Vec<PlayerSlotStart>,
}

impl SessionHeader {
    /// Segment `segment`'s seed, `BLAKE3(domain ‖ u32 segment ‖ server seed ‖ contributions of the
    /// players who started, in player slot order)`, of `server_seed`, that segment's seed of the
    /// committed chain, as `ServerSeeds::commits_to` checks once for every segment.
    pub fn segment_seed(&self, segment: u32, server_seed: &ServerSeed) -> SegmentSeed {
        let mut hasher = DomainHasher::new(SEGMENT_SEED_DOMAIN);
        hasher
            .update(&segment.to_le_bytes())
            .update(server_seed.as_bytes());
        for (_, delegation) in self.players() {
            hasher.update(delegation.terms().seed_contribution.as_bytes());
        }
        hasher.finish()
    }

    /// The players who started, by player slot, with their delegations.
    pub fn players(&self) -> impl Iterator<Item = (PlayerSlot, &Delegation)> {
        (0..)
            .zip(&self.player_slots)
            .filter_map(|(player_slot, start)| match start {
                PlayerSlotStart::Player(delegation) => {
                    Some((PlayerSlot::new(player_slot), &**delegation))
                }
                PlayerSlotStart::Bot | PlayerSlotStart::Open => None,
            })
    }
}
