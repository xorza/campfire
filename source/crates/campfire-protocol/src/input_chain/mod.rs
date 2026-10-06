use blake3::Hasher;
use campfire_common::{PlayerSlot, Tick};
use secp256k1::{Keypair, Secp256k1, Signing, Verification, XOnlyPublicKey};
use serde::{Deserialize, Serialize};

use crate::input_hash::InputHash;
use crate::player_input::PlayerInput;
use crate::session_id::SessionId;
use crate::signature::Signature;

/// Starts every input hash, so no other BLAKE3 use can produce a chain link.
const HASH_DOMAIN: &[u8] = b"campfire/input-hash/v1";
/// Starts every signed chain head, so no other signature of the session key counts as one.
const SIGNATURE_DOMAIN: &[u8] = b"campfire/input/v1";
const HEAD_MESSAGE_LEN: usize = SIGNATURE_DOMAIN.len() + 32 + 4 + 8 + 32;

/// A player's input chain: the hash its next input links to, and that input's seq. The player
/// extends it with each input it sends, and the log extends its own copy with each input it
/// records, so both reach the same head, which the player's signature covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputChain {
    slot: PlayerSlot,
    head: InputHash,
    next_seq: u64,
}

impl InputChain {
    /// A chain with no input yet: the first one links to `root`.
    pub const fn new(slot: PlayerSlot, root: InputHash) -> InputChain {
        InputChain {
            slot,
            head: root,
            next_seq: 0,
        }
    }

    /// A chain whose next input is the `next_seq`th, linking to `head`: as the log holds it, for
    /// a player who takes their chain up again from there.
    pub const fn resume(slot: PlayerSlot, head: InputHash, next_seq: u64) -> InputChain {
        InputChain {
            slot,
            head,
            next_seq,
        }
    }

    /// How many inputs the chain holds: the seq of its next.
    pub const fn next_seq(&self) -> u64 {
        self.next_seq
    }

    pub const fn slot(&self) -> PlayerSlot {
        self.slot
    }

    /// The hash of the last input, or the root before the first.
    pub const fn head(&self) -> InputHash {
        self.head
    }

    /// The player's next input, stamped for `stamp`. The head moves on to `BLAKE3(domain ‖
    /// previous head ‖ u32 slot ‖ u64 seq ‖ u64 stamp ‖ payload)`, little-endian, seq counting
    /// the player's inputs from 0; the payload comes last, so it needs no length.
    pub fn extend<'a>(&mut self, stamp: Tick, payload: &'a [u8]) -> PlayerInput<'a> {
        let mut hasher = Hasher::new();
        hasher
            .update(HASH_DOMAIN)
            .update(self.head.as_bytes())
            .update(&self.slot.get().to_le_bytes())
            .update(&self.next_seq.to_le_bytes())
            .update(&stamp.get().to_le_bytes())
            .update(payload);
        self.head = InputHash::new(*hasher.finalize().as_bytes());
        self.next_seq = self.next_seq.checked_add(1).expect("input seq exhausted");
        PlayerInput {
            slot: self.slot,
            stamp,
            payload,
        }
    }

    /// The session key's signature over the chain head, with BIP-340's auxiliary randomness
    /// `aux`. The chain holds at least one input: a packet ends with one.
    pub fn sign<C: Signing>(
        &self,
        secp: &Secp256k1<C>,
        session_key: &Keypair,
        session_id: SessionId,
        aux: &[u8; 32],
    ) -> Signature {
        Signature::sign(secp, session_key, &self.head_message(session_id), aux)
    }

    /// Whether `signature` is `session_key`'s over the chain head.
    pub(crate) fn signed_by<C: Verification>(
        &self,
        secp: &Secp256k1<C>,
        session_key: &XOnlyPublicKey,
        session_id: SessionId,
        signature: &Signature,
    ) -> bool {
        signature.verifies(secp, session_key, &self.head_message(session_id))
    }

    /// `domain ‖ session id ‖ u32 slot ‖ u64 seq ‖ head`, little-endian, where seq is the last
    /// input's.
    fn head_message(&self, session_id: SessionId) -> [u8; HEAD_MESSAGE_LEN] {
        let seq = self
            .next_seq
            .checked_sub(1)
            .expect("a signed chain head follows an input");
        let mut message = [0; HEAD_MESSAGE_LEN];
        let mut at = 0;
        for part in [
            SIGNATURE_DOMAIN,
            session_id.as_bytes(),
            &self.slot.get().to_le_bytes(),
            &seq.to_le_bytes(),
            self.head.as_bytes(),
        ] {
            message[at..at + part.len()].copy_from_slice(part);
            at += part.len();
        }
        debug_assert_eq!(at, HEAD_MESSAGE_LEN, "the parts fill the message");
        message
    }
}

#[cfg(feature = "bench")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
