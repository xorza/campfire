use secp256k1::{Keypair, Secp256k1, Signing, Verification, XOnlyPublicKey, schnorr};

use crate::chain_signature::ChainSignature;
use crate::input_hash::InputHash;
use crate::player_input::PlayerInput;
use crate::player_slot::PlayerSlot;
use crate::session_id::SessionId;
use crate::session_log::error::InputError;

/// Starts every signed chain head, so no other signature of the session key counts as one.
const SIGNATURE_DOMAIN: &[u8] = b"campfire/input/v1";
const HEAD_MESSAGE_LEN: usize = SIGNATURE_DOMAIN.len() + 32 + 4 + 8 + 32;

/// A player's input chain: the hash its next input links to, and that input's seq. The player
/// extends it with each input it sends; the log accepts each input it records against its own
/// copy, so both sides move on by the same rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    pub const fn slot(&self) -> PlayerSlot {
        self.slot
    }

    /// The player's next input, stamped for `stamp`; the chain moves on to it.
    pub fn extend<'a>(&mut self, stamp: u64, payload: &'a [u8]) -> PlayerInput<'a> {
        let input = PlayerInput {
            slot: self.slot,
            seq: self.next_seq,
            stamp,
            previous: self.head,
            payload,
        };
        self.move_on(&input);
        input
    }

    /// Moves on to `input` when it is the chain's next one; unchanged otherwise.
    pub(crate) fn accept(&mut self, input: &PlayerInput<'_>) -> Result<(), InputError> {
        debug_assert_eq!(
            input.slot, self.slot,
            "a chain accepts its own player's inputs"
        );
        if input.previous != self.head {
            return Err(InputError::BrokenLink);
        }
        if input.seq != self.next_seq {
            return Err(InputError::WrongSeq {
                expected: self.next_seq,
            });
        }
        self.move_on(input);
        Ok(())
    }

    /// The session key's signature over the chain head, with BIP-340's auxiliary randomness
    /// `aux`. The chain holds at least one input: a packet ends with one.
    pub fn sign<C: Signing>(
        &self,
        secp: &Secp256k1<C>,
        session_key: &Keypair,
        session_id: SessionId,
        aux: &[u8; 32],
    ) -> ChainSignature {
        let signature =
            secp.sign_schnorr_with_aux_rand(&self.head_message(session_id), session_key, aux);
        ChainSignature::from_bytes(signature.to_byte_array())
    }

    /// Whether `signature` is `session_key`'s over the chain head.
    pub(crate) fn signed_by<C: Verification>(
        &self,
        secp: &Secp256k1<C>,
        session_key: &XOnlyPublicKey,
        session_id: SessionId,
        signature: &ChainSignature,
    ) -> bool {
        secp.verify_schnorr(
            &schnorr::Signature::from_byte_array(signature.to_bytes()),
            &self.head_message(session_id),
            session_key,
        )
        .is_ok()
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

    fn move_on(&mut self, input: &PlayerInput<'_>) {
        self.head = input.hash();
        self.next_seq = self.next_seq.checked_add(1).expect("input seq exhausted");
    }
}

#[cfg(feature = "bench")]
pub(crate) mod bench;
