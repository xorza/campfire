use campfire_protocol::{InputHash, PlayerInput, PlayerSlot};
use serde::{Deserialize, Serialize};

/// A player input as the client sends it. The server knows the sender's slot from the
/// connection, so the message does not carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputMessage {
    pub seq: u64,
    pub stamp: u64,
    pub previous: [u8; 32],
    pub payload: Vec<u8>,
}

impl InputMessage {
    pub fn new(input: &PlayerInput<'_>) -> InputMessage {
        InputMessage {
            seq: input.seq,
            stamp: input.stamp,
            previous: *input.previous.as_bytes(),
            payload: input.payload.to_vec(),
        }
    }

    /// The input of the player in `slot`.
    pub fn input(&self, slot: PlayerSlot) -> PlayerInput<'_> {
        PlayerInput {
            slot,
            seq: self.seq,
            stamp: self.stamp,
            previous: InputHash::new(self.previous),
            payload: &self.payload,
        }
    }
}
