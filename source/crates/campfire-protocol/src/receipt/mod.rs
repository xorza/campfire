use campfire_common::{PlayerSlot, Tick};
use secp256k1::{Keypair, Secp256k1, Signing, Verification, XOnlyPublicKey};
use serde::{Deserialize, Serialize};

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
    pub delegation: [u8; 32],
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
            &self.delegation,
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
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = FILE_TAG.to_vec();
        postcard::to_io(self, &mut bytes).expect("postcard into a Vec cannot fail");
        bytes
    }

    /// The receipt of a file's `bytes`; an error for bytes that `encode` did not write.
    pub fn decode(bytes: &[u8]) -> Result<SignedReceipt, ReceiptFileError> {
        let rest = bytes
            .strip_prefix(FILE_TAG)
            .ok_or(ReceiptFileError::NotReceipt)?;
        let (receipt, rest) = postcard::take_from_bytes::<SignedReceipt>(rest)
            .map_err(ReceiptFileError::Malformed)?;
        if !rest.is_empty() {
            return Err(ReceiptFileError::Trailing);
        }
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use secp256k1::SecretKey;

    use super::*;

    #[test]
    fn a_receipt_round_trips_and_holds_only_under_its_key_over_its_head() {
        let secp = Secp256k1::new();
        let server =
            Keypair::from_secret_key(&secp, &SecretKey::from_byte_array(&[41; 32]).unwrap());
        let key = server.x_only_public_key().0;
        let receipt = Receipt {
            session_id: SessionId::new([8; 32]),
            slot: PlayerSlot::new(1),
            delegation: [2; 32],
            tick: Tick::new(90),
            seq: 7,
            head: InputHash::new([3; 32]),
        };
        let signed = SignedReceipt {
            receipt,
            signature: receipt.sign(&secp, &server, &[0; 32]),
        };
        let bytes = signed.encode();
        assert_eq!(SignedReceipt::decode(&bytes), Ok(signed));
        assert_eq!(
            SignedReceipt::decode(&bytes[1..]),
            Err(ReceiptFileError::NotReceipt)
        );
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            SignedReceipt::decode(&trailing),
            Err(ReceiptFileError::Trailing)
        );
        assert!(matches!(
            SignedReceipt::decode(&bytes[..bytes.len() - 1]),
            Err(ReceiptFileError::Malformed(_))
        ));
        assert!(receipt.signed_by(&secp, &key, &signed.signature));
        let stranger =
            Keypair::from_secret_key(&secp, &SecretKey::from_byte_array(&[42; 32]).unwrap());
        assert!(!receipt.signed_by(&secp, &stranger.x_only_public_key().0, &signed.signature));
        let others = [
            Receipt {
                head: InputHash::new([4; 32]),
                ..receipt
            },
            Receipt { seq: 8, ..receipt },
            Receipt {
                delegation: [5; 32],
                ..receipt
            },
            Receipt {
                slot: PlayerSlot::new(0),
                ..receipt
            },
            Receipt {
                tick: Tick::new(91),
                ..receipt
            },
            Receipt {
                session_id: SessionId::new([9; 32]),
                ..receipt
            },
        ];
        for other in others {
            assert!(
                !other.signed_by(&secp, &key, &signed.signature),
                "{other:?}"
            );
        }
    }
}
