use campfire_common::{DomainHasher, HashDomain, Seat, SegmentSeed};

use crate::delegation::Delegation;
use crate::seat_start::SeatStart;
use crate::server_seed::ServerSeed;
use crate::session_terms::SessionTerms;

/// Starts the segment seed, so no other BLAKE3 use can produce one.
const SEGMENT_SEED_DOMAIN: HashDomain<SegmentSeed> = HashDomain::new("campfire/segment-seed/v1");

/// What the log fixes before the first tick: the session's terms, and how each of its seats
/// starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHeader {
    pub terms: SessionTerms,
    /// Each seat's start, by seat: a player's delegation, which lets their session key sign
    /// their chain heads, whose id their first input links to, and which carries their seed
    /// contribution; a bot; or open.
    pub seats: Vec<SeatStart>,
}

impl SessionHeader {
    /// Segment `segment`'s seed, `BLAKE3(domain ‖ u32 segment ‖ server seed ‖ contributions of
    /// the players who started, in seat order)`, of `server_seed`, that segment's seed of the
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

    /// The players who started, by seat, with their delegations.
    pub fn players(&self) -> impl Iterator<Item = (Seat, &Delegation)> {
        (0..)
            .zip(&self.seats)
            .filter_map(|(seat, start)| match start {
                SeatStart::Player(delegation) => Some((Seat::new(seat), &**delegation)),
                SeatStart::Bot | SeatStart::Open => None,
            })
    }
}
