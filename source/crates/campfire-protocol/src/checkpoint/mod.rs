use campfire_common::{StateHash, Tick};
use secp256k1::{Keypair, Secp256k1, Signing, Verification, XOnlyPublicKey};
use serde::{Deserialize, Serialize};

use crate::checkpoint::error::CheckpointDecodeError;
use crate::checkpoint::log_carry::{CarryWire, LogCarry};
use crate::decoded::Decoded;
use crate::session_id::SessionId;
use crate::signature::Signature;
use crate::snapshot_fingerprint::SnapshotFingerprint;

pub(crate) mod checkpoint_begun;
pub(crate) mod error;
pub(crate) mod log_carry;

/// Starts every signed checkpoint record, so no other signature of the server key counts as one.
const SIGNATURE_DOMAIN: &[u8] = b"campfire/checkpoint/v1";

/// A checkpoint record, which starts segment `segment` at the boundary before tick `tick`: the
/// state hash there, the fingerprint of the snapshot of that state, and the log's own state there.
/// The server signs it, and the log ends the segment before it there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub segment: u32,
    pub tick: Tick,
    pub state_hash: StateHash,
    pub snapshot: SnapshotFingerprint,
    pub carry: LogCarry,
}

/// A `Checkpoint` as it is signed and logged.
#[derive(Debug, Serialize, Deserialize)]
struct Wire<'a> {
    segment: u32,
    tick: Tick,
    state_hash: StateHash,
    snapshot: SnapshotFingerprint,
    #[serde(borrow)]
    carry: CarryWire<'a>,
}

impl Checkpoint {
    /// Appends its postcard bytes, as it is signed and logged.
    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        let wire = Wire {
            segment: self.segment,
            tick: self.tick,
            state_hash: self.state_hash,
            snapshot: self.snapshot,
            carry: self.carry.wire(),
        };
        postcard::to_io(&wire, out).expect("postcard into a Vec cannot fail");
    }

    /// The record at the front of `bytes`, and the bytes after it; an error for bytes that do
    /// not decode, and for a delegation that does not parse.
    pub(crate) fn take(bytes: &[u8]) -> Result<Decoded<'_, Checkpoint>, CheckpointDecodeError> {
        let (wire, rest) = postcard::take_from_bytes::<Wire<'_>>(bytes)
            .map_err(CheckpointDecodeError::Malformed)?;
        let record = Checkpoint {
            segment: wire.segment,
            tick: wire.tick,
            state_hash: wire.state_hash,
            snapshot: wire.snapshot,
            carry: LogCarry::from_wire(wire.carry)?,
        };
        Ok(Decoded {
            value: record,
            rest,
        })
    }

    /// The server key's signature over the record, with BIP-340's auxiliary randomness `aux`.
    pub fn sign<C: Signing>(
        &self,
        secp: &Secp256k1<C>,
        server_key: &Keypair,
        session_id: SessionId,
        aux: &[u8; 32],
    ) -> Signature {
        let mut message = Vec::new();
        self.write_message(session_id, &mut message);
        Signature::sign(secp, server_key, &message, aux)
    }

    /// Whether `signature` is `server_key`'s over the record, its message written into `message`.
    pub(crate) fn signed_by<C: Verification>(
        &self,
        secp: &Secp256k1<C>,
        server_key: &XOnlyPublicKey,
        session_id: SessionId,
        signature: &Signature,
        message: &mut Vec<u8>,
    ) -> bool {
        self.write_message(session_id, message);
        signature.verifies(secp, server_key, message)
    }

    /// Writes `domain ‖ session id ‖ postcard of the record` into `message`, in place of what
    /// it held.
    fn write_message(&self, session_id: SessionId, message: &mut Vec<u8>) {
        message.clear();
        message.extend_from_slice(SIGNATURE_DOMAIN);
        message.extend_from_slice(session_id.as_bytes());
        self.encode(message);
    }
}

#[cfg(test)]
mod tests;
