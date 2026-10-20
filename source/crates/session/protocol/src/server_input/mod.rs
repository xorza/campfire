use campfire_codec::{Binary, Decoded};
use campfire_common::{AfterLeave, Seat, Tick};
use serde::{Deserialize, Serialize};

use crate::bytes::Bytes;
use crate::delegation::Delegation;
use crate::server_input::error::ServerInputDecodeError;
use crate::session_id::SessionId;
use crate::signed::{SignatureDomain, Signed};

pub(crate) mod error;

/// An input the server makes and signs with its key, logged before the tick it applies in: a
/// bot's commands, and each change of who controls a seat or of a player's link. A bot's payload
/// is borrowed, from where the server wrote it or the bytes it was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerInput<'a> {
    /// Commands of the seat's bot, in the same format as a player's payload.
    Bot { seat: Seat, payload: &'a [u8] },
    /// The player of `delegation` takes the seat, their chain starting from its id.
    Join { seat: Seat, delegation: Delegation },
    /// The seat's player signs with `delegation`'s session key from now on, their chain going
    /// on: the same main key.
    Renew { seat: Seat, delegation: Delegation },
    /// The seat's player left, for `reason`, and the seat became `becomes`.
    Leave {
        seat: Seat,
        reason: LeaveReason,
        becomes: AfterLeave,
    },
    /// The seat's player's link came back.
    Connected { seat: Seat },
    /// The seat's player's link failed.
    Disconnected { seat: Seat },
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
        seat: u32,
        #[serde(borrow)]
        payload: Bytes<'a>,
    },
    Join {
        seat: u32,
        delegation: &'a str,
    },
    Renew {
        seat: u32,
        delegation: &'a str,
    },
    Leave {
        seat: u32,
        reason: LeaveReason,
        becomes: AfterLeave,
    },
    Connected {
        seat: u32,
    },
    Disconnected {
        seat: u32,
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
    /// The seat it is about.
    pub const fn seat(&self) -> Seat {
        match self {
            ServerInput::Bot { seat, .. }
            | ServerInput::Join { seat, .. }
            | ServerInput::Renew { seat, .. }
            | ServerInput::Leave { seat, .. }
            | ServerInput::Connected { seat }
            | ServerInput::Disconnected { seat } => *seat,
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
                seat,
                payload: Bytes(payload),
            } => ServerInput::Bot {
                seat: Seat::new(seat),
                payload,
            },
            Wire::Join {
                seat,
                delegation: json,
            } => ServerInput::Join {
                seat: Seat::new(seat),
                delegation: delegation(json)?,
            },
            Wire::Renew {
                seat,
                delegation: json,
            } => ServerInput::Renew {
                seat: Seat::new(seat),
                delegation: delegation(json)?,
            },
            Wire::Leave {
                seat,
                reason,
                becomes,
            } => ServerInput::Leave {
                seat: Seat::new(seat),
                reason,
                becomes,
            },
            Wire::Connected { seat } => ServerInput::Connected {
                seat: Seat::new(seat),
            },
            Wire::Disconnected { seat } => ServerInput::Disconnected {
                seat: Seat::new(seat),
            },
        };
        Ok(Decoded { value: input, rest })
    }

    fn wire(&self) -> Wire<'_> {
        match self {
            ServerInput::Bot { seat, payload } => Wire::Bot {
                seat: seat.get(),
                payload: Bytes(payload),
            },
            ServerInput::Join { seat, delegation } => Wire::Join {
                seat: seat.get(),
                delegation: delegation.json(),
            },
            ServerInput::Renew { seat, delegation } => Wire::Renew {
                seat: seat.get(),
                delegation: delegation.json(),
            },
            ServerInput::Leave {
                seat,
                reason,
                becomes,
            } => Wire::Leave {
                seat: seat.get(),
                reason: *reason,
                becomes: *becomes,
            },
            ServerInput::Connected { seat } => Wire::Connected { seat: seat.get() },
            ServerInput::Disconnected { seat } => Wire::Disconnected { seat: seat.get() },
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
