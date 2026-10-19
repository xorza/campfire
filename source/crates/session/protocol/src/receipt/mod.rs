use campfire_codec::{Binary, TaggedError};
use campfire_common::{Seat, Tick};
use serde::{Deserialize, Serialize};

use crate::delegation::delegation_id::DelegationId;
use crate::input_hash::InputHash;
use crate::session_id::SessionId;
use crate::signature::Signature;
use crate::signed::{SignatureDomain, Signed};

/// Starts every receipt file and states its version, so other bytes are refused at once.
const FILE_TAG: &[u8] = b"campfire/receipt-file/v1";

/// The server's word that it logged a player's inputs up to seq `seq`, whose chain head is
/// `head`, durably: their journal records were synced, so no crash loses them. It names the
/// session, the seat, the delegation whose key signed the chain head then, and the tick it was
/// made in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub session_id: SessionId,
    pub seat: Seat,
    pub delegation: DelegationId,
    pub tick: Tick,
    pub seq: u64,
    pub head: InputHash,
}

/// A receipt with the server key's signature over it, as a client keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedReceipt {
    pub receipt: Receipt,
    pub signature: Signature,
}

/// `domain ‖ session id ‖ u32 seat ‖ delegation id ‖ u64 tick ‖ u64 seq ‖ head`, little-endian,
/// as the server key signs a receipt.
impl Signed for Receipt {
    const DOMAIN: SignatureDomain = SignatureDomain::new("campfire/receipt/v1");

    fn write_body(&self, message: &mut Vec<u8>) {
        message.extend_from_slice(self.session_id.as_bytes());
        message.extend_from_slice(&self.seat.get().to_le_bytes());
        message.extend_from_slice(self.delegation.as_bytes());
        message.extend_from_slice(&self.tick.get().to_le_bytes());
        message.extend_from_slice(&self.seq.to_le_bytes());
        message.extend_from_slice(self.head.as_bytes());
    }
}

impl SignedReceipt {
    /// The file's bytes: the tag, then the receipt and its signature in postcard.
    pub fn encode(&self, out: &mut Vec<u8>) {
        Binary::encode_tagged(FILE_TAG, self, out);
    }

    /// The receipt of a file's `bytes`; an error for bytes that `encode` did not write.
    pub fn decode(bytes: &[u8]) -> Result<Self, TaggedError> {
        Binary::decode_tagged(FILE_TAG, bytes)
    }
}

#[cfg(test)]
mod tests;
