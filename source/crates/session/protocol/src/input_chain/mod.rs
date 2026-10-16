use campfire_common::{DomainHasher, HashDomain, PlayerSlot, Tick};
use secp256k1::XOnlyPublicKey;
use serde::{Deserialize, Serialize};

use crate::delegation::delegation_id::DelegationId;
use crate::input_hash::InputHash;
use crate::player_input::PlayerInput;
use crate::server_input::InputPlace;
use crate::session_id::SessionId;
use crate::signature::Signature;
use crate::signer::Signer;
use crate::verifier::Verifier;

/// Starts every input hash, so no other BLAKE3 use can produce a chain link.
const HASH_DOMAIN: HashDomain<InputHash> = HashDomain::new("campfire/input-hash/v1");
/// Starts every root of a chain a join starts, so no other BLAKE3 use can produce one.
const JOIN_DOMAIN: HashDomain<InputHash> = HashDomain::new("campfire/join-root/v1");
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
    pub const fn new(slot: PlayerSlot, root: InputHash) -> Self {
        Self {
            slot,
            head: root,
            next_seq: 0,
        }
    }

    /// The chain a `Join` at `place` starts for the player of `delegation`: its root is
    /// `BLAKE3(domain ‖ delegation id ‖ u64 tick ‖ u32 index)`, little-endian, of the join's
    /// place, which no other join of the log holds. A player who joins again with the same
    /// delegation so starts a chain that none of their earlier packets extends, and the host
    /// cannot log those again.
    pub fn joined(slot: PlayerSlot, delegation: &DelegationId, place: InputPlace) -> Self {
        let mut hasher = DomainHasher::new(JOIN_DOMAIN);
        hasher
            .update(delegation.as_bytes())
            .update(&place.tick.get().to_le_bytes())
            .update(&place.index.to_le_bytes());
        Self::new(slot, hasher.finish())
    }

    /// A chain whose next input is the `next_seq`th, linking to `head`: as the log holds it, for
    /// a player who takes their chain up again from there.
    pub const fn resume(slot: PlayerSlot, head: InputHash, next_seq: u64) -> Self {
        Self {
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
        let mut hasher = DomainHasher::new(HASH_DOMAIN);
        hasher
            .update(self.head.as_bytes())
            .update(&self.slot.get().to_le_bytes())
            .update(&self.next_seq.to_le_bytes())
            .update(&stamp.get().to_le_bytes())
            .update(payload);
        self.head = hasher.finish();
        self.next_seq = self.next_seq.checked_add(1).expect("input seq exhausted");
        PlayerInput {
            slot: self.slot,
            stamp,
            payload,
        }
    }

    /// The session key's signature over the chain head, `session`'s. The chain holds at least
    /// one input: a packet ends with one.
    pub fn sign(&self, session: &Signer, session_id: SessionId) -> Signature {
        session.sign(&self.head_message(session_id))
    }

    /// Whether `signature` is `session_key`'s over the chain head.
    pub(crate) fn signed_by(
        &self,
        verifier: &Verifier,
        session_key: &XOnlyPublicKey,
        session_id: SessionId,
        signature: &Signature,
    ) -> bool {
        verifier.verifies(*signature, session_key, &self.head_message(session_id))
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

#[cfg(feature = "internals")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
