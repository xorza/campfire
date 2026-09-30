use blake3::Hasher;
use serde::{Deserialize, Serialize, Serializer};

use crate::input_hash::InputHash;
use crate::player_slot::PlayerSlot;

/// Starts every input hash, so no other BLAKE3 use can produce a chain link.
const HASH_DOMAIN: &[u8] = b"campfire/input-hash/v1";

/// One input of a player. `previous` is the hash of the player's input before it, or for the first
/// one the player's chain root, so a later link covers every earlier input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInput<'a> {
    pub slot: PlayerSlot,
    /// Counts the player's inputs from 0.
    pub seq: u64,
    /// The tick the player wants the input applied in.
    pub stamp: u64,
    pub previous: InputHash,
    #[serde(serialize_with = "serialize_payload")]
    pub payload: &'a [u8],
}

impl PlayerInput<'_> {
    /// The hash the player's next input carries: `domain ‖ previous ‖ u32 slot ‖ u64 seq ‖
    /// u64 stamp ‖ payload`, little-endian. The payload comes last, so it needs no length.
    pub fn hash(&self) -> InputHash {
        let mut hasher = Hasher::new();
        hasher
            .update(HASH_DOMAIN)
            .update(self.previous.as_bytes())
            .update(&self.slot.get().to_le_bytes())
            .update(&self.seq.to_le_bytes())
            .update(&self.stamp.to_le_bytes())
            .update(self.payload);
        InputHash::new(*hasher.finalize().as_bytes())
    }
}

/// As postcard bytes, a length and the bytes; a plain slice would go through serde's element by
/// element sequence to reach the same encoding.
fn serialize_payload<S: Serializer>(payload: &&[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(payload)
}
