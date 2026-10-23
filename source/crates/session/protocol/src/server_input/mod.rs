use campfire_codec::{Binary, Decoded};
use campfire_common::{AfterLeave, PlayerSlot, Tick};
use serde::{Deserialize, Serialize};

use crate::bytes::Bytes;
use crate::delegation::Delegation;
use crate::server_input::error::ServerInputDecodeError;
use crate::session_id::SessionId;
use crate::signed::{SignatureDomain, Signed};

pub(crate) mod error;

/// An input the server makes and signs with its key, logged before the tick it applies in: a bot's
/// commands, and each change of who controls a player slot or of a player's link. A bot's payload
/// is borrowed, from where the server wrote it or the bytes it was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerInput<'a> {
    /// Commands of the player slot's bot, in the same format as a player's payload.
    Bot {
        player_slot: PlayerSlot,
        payload: &'a [u8],
    },
    /// The player of `delegation` takes the player slot, their chain starting from its id.
    Join {
        player_slot: PlayerSlot,
        delegation: Delegation,
    },
    /// The player slot's player signs with `delegation`'s session key from now on, their chain
    /// going on: the same main key.
    Renew {
        player_slot: PlayerSlot,
        delegation: Delegation,
    },
    /// The player slot's player left, for `reason`, and the player slot became `becomes`.
    Leave {
        player_slot: PlayerSlot,
        reason: LeaveReason,
        becomes: AfterLeave,
    },
    /// The player slot's player's link came back.
    Connected { player_slot: PlayerSlot },
    /// The player slot's player's link failed.
    Disconnected { player_slot: PlayerSlot },
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
        player_slot: u32,
        #[serde(borrow)]
        payload: Bytes<'a>,
    },
    Join {
        player_slot: u32,
        delegation: &'a str,
    },
    Renew {
        player_slot: u32,
        delegation: &'a str,
    },
    Leave {
        player_slot: u32,
        reason: LeaveReason,
        becomes: AfterLeave,
    },
    Connected {
        player_slot: u32,
    },
    Disconnected {
        player_slot: u32,
    },
}

/// A server input as the server key signs it, logged at `place` in session `session_id`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlacedInput<'i> {
    pub input: &'i ServerInput<'i>,
    pub session_id: SessionId,
    pub place: InputPlace,
}

/// A server input as `PlacedInput` signs it, of its encoding `input`, which the log wrote once:
/// the same message, with no second encode of the input (08-sessions.md, A record is encoded
/// once).
#[derive(Debug, Clone, Copy)]
pub(crate) struct EncodedPlace<'a> {
    pub(crate) session_id: SessionId,
    pub(crate) place: InputPlace,
    pub(crate) input: &'a [u8],
}

/// `domain ‖ session id ‖ u64 tick ‖ u32 index ‖ input`, little-endian.
impl Signed for PlacedInput<'_> {
    const DOMAIN: SignatureDomain = SignatureDomain::new("campfire/server-input/v1");

    fn write_body(&self, message: &mut Vec<u8>) {
        write_place(message, self.session_id, self.place);
        self.input.encode(message);
    }
}

impl Signed for EncodedPlace<'_> {
    const DOMAIN: SignatureDomain = PlacedInput::DOMAIN;

    fn write_body(&self, message: &mut Vec<u8>) {
        write_place(message, self.session_id, self.place);
        message.extend_from_slice(self.input);
    }
}

/// Writes the session id and the place a server input's message holds before the input.
fn write_place(message: &mut Vec<u8>, session_id: SessionId, place: InputPlace) {
    message.extend_from_slice(session_id.as_bytes());
    message.extend_from_slice(&place.tick.get().to_le_bytes());
    message.extend_from_slice(&place.index.to_le_bytes());
}

impl<'a> ServerInput<'a> {
    /// The player slot it is about.
    pub const fn player_slot(&self) -> PlayerSlot {
        match self {
            ServerInput::Bot { player_slot, .. }
            | ServerInput::Join { player_slot, .. }
            | ServerInput::Renew { player_slot, .. }
            | ServerInput::Leave { player_slot, .. }
            | ServerInput::Connected { player_slot }
            | ServerInput::Disconnected { player_slot } => *player_slot,
        }
    }

    /// Appends its postcard bytes, as it is signed and logged.
    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        Binary::encode_into(&self.wire(), out);
    }

    /// The input at the front of `bytes`, and the bytes after it; an error for bytes that do not
    /// decode, and for a delegation that does not parse.
    pub(crate) fn take(bytes: &'a [u8]) -> Result<Decoded<'a, Self>, ServerInputDecodeError> {
        let Decoded { value: wire, rest } =
            Binary::take::<Wire<'a>>(bytes).map_err(ServerInputDecodeError::Malformed)?;
        let delegation =
            |json: &str| Delegation::parse(json).map_err(ServerInputDecodeError::Delegation);
        let input = match wire {
            Wire::Bot {
                player_slot,
                payload: Bytes(payload),
            } => ServerInput::Bot {
                player_slot: PlayerSlot::new(player_slot),
                payload,
            },
            Wire::Join {
                player_slot,
                delegation: json,
            } => ServerInput::Join {
                player_slot: PlayerSlot::new(player_slot),
                delegation: delegation(json)?,
            },
            Wire::Renew {
                player_slot,
                delegation: json,
            } => ServerInput::Renew {
                player_slot: PlayerSlot::new(player_slot),
                delegation: delegation(json)?,
            },
            Wire::Leave {
                player_slot,
                reason,
                becomes,
            } => ServerInput::Leave {
                player_slot: PlayerSlot::new(player_slot),
                reason,
                becomes,
            },
            Wire::Connected { player_slot } => ServerInput::Connected {
                player_slot: PlayerSlot::new(player_slot),
            },
            Wire::Disconnected { player_slot } => ServerInput::Disconnected {
                player_slot: PlayerSlot::new(player_slot),
            },
        };
        Ok(Decoded { value: input, rest })
    }

    fn wire(&self) -> Wire<'_> {
        match self {
            ServerInput::Bot {
                player_slot,
                payload,
            } => Wire::Bot {
                player_slot: player_slot.get(),
                payload: Bytes(payload),
            },
            ServerInput::Join {
                player_slot,
                delegation,
            } => Wire::Join {
                player_slot: player_slot.get(),
                delegation: delegation.json(),
            },
            ServerInput::Renew {
                player_slot,
                delegation,
            } => Wire::Renew {
                player_slot: player_slot.get(),
                delegation: delegation.json(),
            },
            ServerInput::Leave {
                player_slot,
                reason,
                becomes,
            } => Wire::Leave {
                player_slot: player_slot.get(),
                reason: *reason,
                becomes: *becomes,
            },
            ServerInput::Connected { player_slot } => Wire::Connected {
                player_slot: player_slot.get(),
            },
            ServerInput::Disconnected { player_slot } => Wire::Disconnected {
                player_slot: player_slot.get(),
            },
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
