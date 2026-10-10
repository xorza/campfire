use campfire_common::{Binary, PlayerSlot, Taken, Tick};
use secp256k1::{Keypair, Secp256k1, Signing, Verification, XOnlyPublicKey};
use serde::{Deserialize, Serialize};

use crate::delegation::delegation_id::DelegationId;
use crate::input_hash::InputHash;
use crate::receipt::error::ReceiptFileError;
use crate::session_id::SessionId;
use crate::signature::Signature;

pub(crate) mod error;

/// Starts every receipt file and states its version, so other bytes are refused at once.
const FILE_TAG: &[u8] = b"campfire/receipt-file/v1";
/// Starts every signed receipt, so no other signature of the server key counts as one.
const SIGNATURE_DOMAIN: &[u8] = b"campfire/receipt/v1";
const MESSAGE_LEN: usize = SIGNATURE_DOMAIN.len() + 32 + 4 + 32 + 8 + 8 + 32;

/// The server's word that it logged a player's inputs up to seq `seq`, whose chain head is
/// `head`, durably: their journal records were synced, so no crash loses them. It names the
/// session, the slot, the delegation whose key signed the chain head then, and the tick it was
/// made in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub session_id: SessionId,
    pub slot: PlayerSlot,
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

impl Receipt {
    /// The server key's signature over the receipt, with BIP-340's auxiliary randomness `aux`.
    pub fn sign<C: Signing>(
        &self,
        secp: &Secp256k1<C>,
        server_key: &Keypair,
        aux: &[u8; 32],
    ) -> Signature {
        Signature::sign(secp, server_key, &self.message(), aux)
    }

    /// Whether `signature` is `server_key`'s over the receipt.
    pub fn signed_by<C: Verification>(
        &self,
        secp: &Secp256k1<C>,
        server_key: &XOnlyPublicKey,
        signature: &Signature,
    ) -> bool {
        signature.verifies(secp, server_key, &self.message())
    }

    /// `domain ‖ session id ‖ u32 slot ‖ delegation id ‖ u64 tick ‖ u64 seq ‖ head`,
    /// little-endian.
    fn message(&self) -> [u8; MESSAGE_LEN] {
        let mut message = [0; MESSAGE_LEN];
        let mut at = 0;
        for part in [
            SIGNATURE_DOMAIN,
            self.session_id.as_bytes(),
            &self.slot.get().to_le_bytes(),
            self.delegation.as_bytes(),
            &self.tick.get().to_le_bytes(),
            &self.seq.to_le_bytes(),
            self.head.as_bytes(),
        ] {
            message[at..at + part.len()].copy_from_slice(part);
            at += part.len();
        }
        debug_assert_eq!(at, MESSAGE_LEN, "the parts fill the message");
        message
    }
}

impl SignedReceipt {
    /// The file's bytes: the tag, then the receipt and its signature in postcard.
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(FILE_TAG);
        Binary::encode_into(self, out);
    }

    /// The receipt of a file's `bytes`; an error for bytes that `encode` did not write.
    pub fn decode(bytes: &[u8]) -> Result<SignedReceipt, ReceiptFileError> {
        let rest = bytes
            .strip_prefix(FILE_TAG)
            .ok_or(ReceiptFileError::NotReceipt)?;
        let Taken {
            value: receipt,
            rest,
        } = Binary::take::<SignedReceipt>(rest).map_err(ReceiptFileError::Malformed)?;
        if !rest.is_empty() {
            return Err(ReceiptFileError::Trailing);
        }
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests;
