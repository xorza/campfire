use campfire_codec::{Binary, Taken};
use campfire_common::{AfterLeave, PlayerSlot, Tick};
use serde::{Deserialize, Serialize};

use crate::bytes::Bytes;
use crate::delegation::Delegation;
use crate::server_input::error::ServerInputDecodeError;
use crate::session_id::SessionId;
use crate::signed::{SignatureDomain, Signed};

pub(crate) mod error;

/// An input the server makes and signs with its key, logged before the tick it applies in: a
/// bot's commands, and each change of who controls a slot or of a player's link. A bot's payload
/// is borrowed, from where the server wrote it or the bytes it was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerInput<'a> {
    /// Commands of the slot's bot, in the same format as a player's payload.
    Bot { slot: PlayerSlot, payload: &'a [u8] },
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

/// A server input as it is signed and logged: a delegation as its JSON.
#[derive(Debug, Serialize, Deserialize)]
enum Wire<'a> {
    Bot {
        slot: u32,
        #[serde(borrow)]
        payload: Bytes<'a>,
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

/// A server input as the server key signs it, logged at `place` in session `session_id`.
#[derive(Debug, Clone, Copy)]
pub struct PlacedInput<'i> {
    pub input: &'i ServerInput<'i>,
    pub session_id: SessionId,
    pub place: InputPlace,
}

/// `domain ‖ session id ‖ u64 tick ‖ u32 index ‖ input`, little-endian.
impl Signed for PlacedInput<'_> {
    const DOMAIN: SignatureDomain = SignatureDomain::new("campfire/server-input/v1");

    fn write_body(&self, message: &mut Vec<u8>) {
        message.extend_from_slice(self.session_id.as_bytes());
        message.extend_from_slice(&self.place.tick.get().to_le_bytes());
        message.extend_from_slice(&self.place.index.to_le_bytes());
        self.input.encode(message);
    }
}

impl<'a> ServerInput<'a> {
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
        Binary::encode_into(&self.wire(), out);
    }

    /// The input at the front of `bytes`, and the bytes after it; an error for bytes that do not
    /// decode, and for a delegation that does not parse.
    pub(crate) fn take(bytes: &'a [u8]) -> Result<Taken<'a, Self>, ServerInputDecodeError> {
        let Taken { value: wire, rest } =
            Binary::take::<Wire<'a>>(bytes).map_err(ServerInputDecodeError::Malformed)?;
        let delegation =
            |json: &str| Delegation::parse(json).map_err(ServerInputDecodeError::Delegation);
        let input = match wire {
            Wire::Bot {
                slot,
                payload: Bytes(payload),
            } => ServerInput::Bot {
                slot: PlayerSlot::new(slot),
                payload,
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
        Ok(Taken { value: input, rest })
    }

    fn wire(&self) -> Wire<'_> {
        match self {
            ServerInput::Bot { slot, payload } => Wire::Bot {
                slot: slot.get(),
                payload: Bytes(payload),
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
