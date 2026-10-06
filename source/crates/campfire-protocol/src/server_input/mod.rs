use campfire_common::{PlayerSlot, Tick};
use secp256k1::{Keypair, Secp256k1, Signing, Verification, XOnlyPublicKey};
use serde::{Deserialize, Serialize};

use crate::delegation::Delegation;
use crate::server_input::error::ServerInputDecodeError;
use crate::session_id::SessionId;
use crate::signature::Signature;

pub(crate) mod error;

/// Starts every signed server input, so no other signature of the server key counts as one.
const SIGNATURE_DOMAIN: &[u8] = b"campfire/server-input/v1";

/// An input the server makes and signs with its key, logged before the tick it applies in: a
/// bot's commands, and each change of who controls a slot or of a player's link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerInput {
    /// Commands of the slot's bot, in the same format as a player's payload.
    Bot { slot: PlayerSlot, payload: Vec<u8> },
    /// The player of `delegation` takes the slot, their chain starting from its id.
    Join {
        slot: PlayerSlot,
        delegation: Delegation,
    },
    /// The slot's player signs with `delegation`'s session key from now on, their chain going
    /// on: the same main key.
    Renew {
        slot: PlayerSlot,
        delegation: Delegation,
    },
    /// The slot's player left, for `reason`, and the slot became `becomes`.
    Leave {
        slot: PlayerSlot,
        reason: LeaveReason,
        becomes: AfterLeave,
    },
    /// The slot's player's link came back.
    Connected { slot: PlayerSlot },
    /// The slot's player's link failed.
    Disconnected { slot: PlayerSlot },
}

/// Why a player left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaveReason {
    /// The player asked to.
    Asked,
    /// The player's link stayed down past the grace period.
    Grace,
}

/// What the slot of a player who left becomes: reserved for them alone, played by a bot, or
/// open, as the mode's `[players] leaver` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AfterLeave {
    Reserve,
    Bot,
    Open,
}

/// A server input as it is signed and logged: a delegation as its JSON.
#[derive(Debug, Serialize, Deserialize)]
enum Wire<'a> {
    Bot {
        slot: u32,
        payload: &'a [u8],
    },
    Join {
        slot: u32,
        delegation: &'a str,
    },
    Renew {
        slot: u32,
        delegation: &'a str,
    },
    Leave {
        slot: u32,
        reason: LeaveReason,
        becomes: AfterLeave,
    },
    Connected {
        slot: u32,
    },
    Disconnected {
        slot: u32,
    },
}

impl ServerInput {
    /// The slot it is about.
    pub const fn slot(&self) -> PlayerSlot {
        match self {
            ServerInput::Bot { slot, .. }
            | ServerInput::Join { slot, .. }
            | ServerInput::Renew { slot, .. }
            | ServerInput::Leave { slot, .. }
            | ServerInput::Connected { slot }
            | ServerInput::Disconnected { slot } => *slot,
        }
    }

    /// Appends its postcard bytes, as it is signed and logged.
    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        postcard::to_io(&self.wire(), out).expect("postcard into a Vec cannot fail");
    }

    /// The input at the front of `bytes`, and the bytes after it; an error for bytes that do not
    /// decode, and for a delegation that does not parse.
    pub(crate) fn take(bytes: &[u8]) -> Result<(ServerInput, &[u8]), ServerInputDecodeError> {
        let (wire, rest) = postcard::take_from_bytes::<Wire<'_>>(bytes)
            .map_err(ServerInputDecodeError::Malformed)?;
        let delegation =
            |json: &str| Delegation::parse(json).map_err(ServerInputDecodeError::Delegation);
        let input = match wire {
            Wire::Bot { slot, payload } => ServerInput::Bot {
                slot: PlayerSlot::new(slot),
                payload: payload.to_vec(),
            },
            Wire::Join {
                slot,
                delegation: json,
            } => ServerInput::Join {
                slot: PlayerSlot::new(slot),
                delegation: delegation(json)?,
            },
            Wire::Renew {
                slot,
                delegation: json,
            } => ServerInput::Renew {
                slot: PlayerSlot::new(slot),
                delegation: delegation(json)?,
            },
            Wire::Leave {
                slot,
                reason,
                becomes,
            } => ServerInput::Leave {
                slot: PlayerSlot::new(slot),
                reason,
                becomes,
            },
            Wire::Connected { slot } => ServerInput::Connected {
                slot: PlayerSlot::new(slot),
            },
            Wire::Disconnected { slot } => ServerInput::Disconnected {
                slot: PlayerSlot::new(slot),
            },
        };
        Ok((input, rest))
    }

    /// The server key's signature over the input, logged before `tick` as its `index`th server
    /// input there, with BIP-340's auxiliary randomness `aux`.
    pub fn sign<C: Signing>(
        &self,
        secp: &Secp256k1<C>,
        server_key: &Keypair,
        session_id: SessionId,
        place: InputPlace,
        aux: &[u8; 32],
    ) -> Signature {
        Signature::sign(secp, server_key, &self.message(session_id, place), aux)
    }

    /// Whether `signature` is `server_key`'s over the input at `place`.
    pub(crate) fn signed_by<C: Verification>(
        &self,
        secp: &Secp256k1<C>,
        server_key: &XOnlyPublicKey,
        session_id: SessionId,
        place: InputPlace,
        signature: &Signature,
    ) -> bool {
        signature.verifies(secp, server_key, &self.message(session_id, place))
    }

    /// `domain ‖ session id ‖ u64 tick ‖ u32 index ‖ input`, little-endian.
    fn message(&self, session_id: SessionId, place: InputPlace) -> Vec<u8> {
        let mut message = Vec::with_capacity(SIGNATURE_DOMAIN.len() + 32 + 8 + 4 + 64);
        message.extend_from_slice(SIGNATURE_DOMAIN);
        message.extend_from_slice(session_id.as_bytes());
        message.extend_from_slice(&place.tick.get().to_le_bytes());
        message.extend_from_slice(&place.index.to_le_bytes());
        self.encode(&mut message);
        message
    }

    fn wire(&self) -> Wire<'_> {
        match self {
            ServerInput::Bot { slot, payload } => Wire::Bot {
                slot: slot.get(),
                payload,
            },
            ServerInput::Join { slot, delegation } => Wire::Join {
                slot: slot.get(),
                delegation: delegation.json(),
            },
            ServerInput::Renew { slot, delegation } => Wire::Renew {
                slot: slot.get(),
                delegation: delegation.json(),
            },
            ServerInput::Leave {
                slot,
                reason,
                becomes,
            } => Wire::Leave {
                slot: slot.get(),
                reason: *reason,
                becomes: *becomes,
            },
            ServerInput::Connected { slot } => Wire::Connected { slot: slot.get() },
            ServerInput::Disconnected { slot } => Wire::Disconnected { slot: slot.get() },
        }
    }
}

/// Where a server input stands in the log: before tick `tick`, the `index`th server input logged
/// there, from 0. Its signature covers both, so no input moves to another tick or place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputPlace {
    pub tick: Tick,
    pub index: u32,
}

#[cfg(test)]
mod tests;
