use campfire_common::{PlayerSlot, Tick};

/// One input of a player, as it is sent and logged: the tick it is for and its payload. Its place
/// in the player's chain, its seq and the hash it links to, is not sent: sender and receiver
/// each compute it from their own copy of the chain, and the signature over the chain head shows
/// that they agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerInput<'a> {
    pub slot: PlayerSlot,
    /// The tick the player wants the input applied in.
    pub stamp: Tick,
    pub payload: &'a [u8],
}
