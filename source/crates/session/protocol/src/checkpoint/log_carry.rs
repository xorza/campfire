use campfire_common::{PlayerSlot, Tick};
use serde::{Deserialize, Serialize};

use crate::bytes::Bytes;
use crate::checkpoint::error::CheckpointDecodeError;
use crate::control::Control;
use crate::delegation::Delegation;
use crate::main_key::MainKey;
use crate::spill::Spill;
use crate::stamp_count::StampCount;

/// The log's own state at a checkpoint's boundary, which a log file that starts at the checkpoint
/// starts from: each player slot's controller, with a player's delegation and chain, the main key
/// of the player who left it last, its stamp counts and its spill; and the inputs logged before the
/// boundary and due from it on, in the order they apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogCarry {
    pub(crate) player_slots: Vec<CarriedPlayerSlot>,
    pub(crate) pending: Vec<CarriedInput>,
}

/// One player slot of a `LogCarry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarriedPlayerSlot {
    pub(crate) control: Control<Box<Delegation>>,
    pub(crate) leaver: Option<MainKey>,
    pub(crate) stamps: StampCount,
    pub(crate) spill: Spill,
}

/// An input logged before the boundary that applies in `tick`, from it on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarriedInput {
    pub(crate) tick: Tick,
    pub(crate) player_slot: PlayerSlot,
    pub(crate) stamp: Tick,
    pub(crate) payload: Vec<u8>,
}

/// A `LogCarry` as it is signed and logged: a delegation as its JSON.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CarryWire<'a> {
    #[serde(borrow)]
    player_slots: Vec<PlayerSlotWire<'a>>,
    #[serde(borrow)]
    pending: Vec<InputWire<'a>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlayerSlotWire<'a> {
    #[serde(borrow)]
    control: Control<&'a str>,
    leaver: Option<MainKey>,
    stamps: StampCount,
    spill: Spill,
}

#[derive(Debug, Serialize, Deserialize)]
struct InputWire<'a> {
    tick: Tick,
    player_slot: PlayerSlot,
    stamp: Tick,
    #[serde(borrow)]
    payload: Bytes<'a>,
}

impl LogCarry {
    pub(crate) fn wire(&self) -> CarryWire<'_> {
        let player_slots = self
            .player_slots
            .iter()
            .map(|player_slot| PlayerSlotWire {
                control: player_slot
                    .control
                    .as_ref()
                    .map(|delegation| delegation.json()),
                leaver: player_slot.leaver,
                stamps: player_slot.stamps,
                spill: player_slot.spill,
            })
            .collect();
        let pending = self
            .pending
            .iter()
            .map(|input| InputWire {
                tick: input.tick,
                player_slot: input.player_slot,
                stamp: input.stamp,
                payload: Bytes(&input.payload),
            })
            .collect();
        CarryWire {
            player_slots,
            pending,
        }
    }

    /// The carry `wire` holds; an error for a delegation that does not parse.
    pub(crate) fn from_wire(wire: CarryWire<'_>) -> Result<Self, CheckpointDecodeError> {
        let mut player_slots = Vec::with_capacity(wire.player_slots.len());
        for (player_slot, carried) in (0..).zip(wire.player_slots) {
            let control = carried.control.try_map(|delegation| {
                Delegation::parse(delegation)
                    .map(Box::new)
                    .map_err(|error| CheckpointDecodeError::Delegation {
                        player_slot: PlayerSlot::new(player_slot),
                        error,
                    })
            })?;
            player_slots.push(CarriedPlayerSlot {
                control,
                leaver: carried.leaver,
                stamps: carried.stamps,
                spill: carried.spill,
            });
        }
        let pending = wire
            .pending
            .into_iter()
            .map(|input| CarriedInput {
                tick: input.tick,
                player_slot: input.player_slot,
                stamp: input.stamp,
                payload: input.payload.0.to_vec(),
            })
            .collect();
        Ok(Self {
            player_slots,
            pending,
        })
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::checkpoint::log_carry::LogCarry;

    impl LogCarry {
        /// A carry of no player slot and no input, for a test of a checkpoint outside a log.
        pub const fn empty() -> Self {
            Self {
                player_slots: Vec::new(),
                pending: Vec::new(),
            }
        }
    }
}
