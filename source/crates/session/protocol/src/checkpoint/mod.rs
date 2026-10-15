use campfire_codec::{Binary, Taken};
use campfire_common::{StateHash, Tick};
use secp256k1::XOnlyPublicKey;
use serde::{Deserialize, Serialize};

use crate::checkpoint::error::CheckpointDecodeError;
use crate::checkpoint::log_carry::{CarryWire, LogCarry};
use crate::session_id::SessionId;
use crate::signature::Signature;
use crate::signer::Signer;
use crate::snapshot_fingerprint::SnapshotFingerprint;
use crate::verifier::Verifier;

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
        Binary::encode_into(&wire, out);
    }

    /// The record at the front of `bytes`, and the bytes after it; an error for bytes that do
    /// not decode, and for a delegation that does not parse.
    pub(crate) fn take(bytes: &[u8]) -> Result<Taken<'_, Checkpoint>, CheckpointDecodeError> {
        let Taken { value: wire, rest } =
            Binary::take::<Wire<'_>>(bytes).map_err(CheckpointDecodeError::Malformed)?;
        let record = Checkpoint {
            segment: wire.segment,
            tick: wire.tick,
            state_hash: wire.state_hash,
            snapshot: wire.snapshot,
            carry: LogCarry::from_wire(wire.carry)?,
        };
        Ok(Taken {
            value: record,
            rest,
        })
    }

    /// The server key's signature over the record, `server`'s.
    pub fn sign(&self, server: &Signer, session_id: SessionId) -> Signature {
        let mut message = Vec::new();
        self.write_message(session_id, &mut message);
        server.sign(&message)
    }

    /// Whether `signature` is `server_key`'s over the record.
    pub(crate) fn signed_by(
        &self,
        verifier: &mut Verifier,
        server_key: &XOnlyPublicKey,
        session_id: SessionId,
        signature: &Signature,
    ) -> bool {
        verifier.verifies_written(*signature, server_key, |message| {
            self.write_message(session_id, message);
        })
    }

    /// Appends `domain ‖ session id ‖ postcard of the record` to `message`.
    fn write_message(&self, session_id: SessionId, message: &mut Vec<u8>) {
        message.extend_from_slice(SIGNATURE_DOMAIN);
        message.extend_from_slice(session_id.as_bytes());
        self.encode(message);
    }
}

#[cfg(test)]
mod tests;
