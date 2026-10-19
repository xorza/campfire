use campfire_common::Seat;

use crate::signature::Signature;

/// A packet of the player of `seat`: `inputs`, each its stamp and payload, in the order sent,
/// and the player's session-key `signature` over the chain head after the last.
#[derive(Debug, Clone, Copy)]
pub struct PlayerPacket<'s, I> {
    pub seat: Seat,
    pub inputs: I,
    pub signature: &'s Signature,
}
