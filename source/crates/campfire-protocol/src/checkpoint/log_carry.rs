use campfire_common::{PlayerSlot, Tick};
use serde::{Deserialize, Serialize};

use crate::bytes::Bytes;
use crate::checkpoint::error::CheckpointDecodeError;
use crate::delegation::Delegation;
use crate::input_chain::InputChain;
use crate::session_log::{Spill, StampCount};

/// The log's own state at a checkpoint's boundary, so a segment verifies from its checkpoint
/// alone: each slot's controller, with a player's delegation and chain, the main key of the
/// player who left it last, its stamp counts and its spill; and the inputs logged before the
/// boundary and due from it on, in the order they apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogCarry {
    pub(crate) slots: Vec<CarriedSlot>,
    pub(crate) pending: Vec<CarriedInput>,
}

/// One slot of a `LogCarry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarriedSlot {
    pub(crate) control: CarriedControl,
    pub(crate) leaver: Option<[u8; 32]>,
    pub(crate) stamps: StampCount,
    pub(crate) spill: Spill,
}

/// Who controls a carried slot: a player, by their current delegation and their chain as logged
/// so far; a bot; no one; or no one but the player who left it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CarriedControl {
    Player {
        delegation: Box<Delegation>,
        chain: InputChain,
    },
    Bot,
    Open,
    Reserved,
}

/// An input logged before the boundary that applies in `tick`, from it on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarriedInput {
    pub(crate) tick: Tick,
    pub(crate) slot: PlayerSlot,
    pub(crate) stamp: Tick,
    pub(crate) payload: Vec<u8>,
}

/// A `LogCarry` as it is signed and logged: a delegation as its JSON.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CarryWire<'a> {
    #[serde(borrow)]
    slots: Vec<SlotWire<'a>>,
    #[serde(borrow)]
    pending: Vec<InputWire<'a>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SlotWire<'a> {
    #[serde(borrow)]
    control: ControlWire<'a>,
    leaver: Option<[u8; 32]>,
    stamps: StampCount,
    spill: Spill,
}

#[derive(Debug, Serialize, Deserialize)]
enum ControlWire<'a> {
    Player {
        delegation: &'a str,
        chain: InputChain,
    },
    Bot,
    Open,
    Reserved,
}

#[derive(Debug, Serialize, Deserialize)]
struct InputWire<'a> {
    tick: Tick,
    slot: PlayerSlot,
    stamp: Tick,
    #[serde(borrow)]
    payload: Bytes<'a>,
}

impl LogCarry {
    pub(crate) fn wire(&self) -> CarryWire<'_> {
        let slots = self
            .slots
            .iter()
            .map(|slot| SlotWire {
                control: match &slot.control {
                    CarriedControl::Player { delegation, chain } => ControlWire::Player {
                        delegation: delegation.json(),
                        chain: *chain,
                    },
                    CarriedControl::Bot => ControlWire::Bot,
                    CarriedControl::Open => ControlWire::Open,
                    CarriedControl::Reserved => ControlWire::Reserved,
                },
                leaver: slot.leaver,
                stamps: slot.stamps,
                spill: slot.spill,
            })
            .collect();
        let pending = self
            .pending
            .iter()
            .map(|input| InputWire {
                tick: input.tick,
                slot: input.slot,
                stamp: input.stamp,
                payload: Bytes(&input.payload),
            })
            .collect();
        CarryWire { slots, pending }
    }

    /// The carry `wire` holds; an error for a delegation that does not parse.
    pub(crate) fn from_wire(wire: CarryWire<'_>) -> Result<LogCarry, CheckpointDecodeError> {
        let mut slots = Vec::with_capacity(wire.slots.len());
        for (slot, carried) in (0..).zip(wire.slots) {
            let control = match carried.control {
                ControlWire::Player { delegation, chain } => CarriedControl::Player {
                    delegation: Box::new(Delegation::parse(delegation).map_err(|error| {
                        CheckpointDecodeError::Delegation {
                            slot: PlayerSlot::new(slot),
                            error,
                        }
                    })?),
                    chain,
                },
                ControlWire::Bot => CarriedControl::Bot,
                ControlWire::Open => CarriedControl::Open,
                ControlWire::Reserved => CarriedControl::Reserved,
            };
            slots.push(CarriedSlot {
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
                slot: input.slot,
                stamp: input.stamp,
                payload: input.payload.0.to_vec(),
            })
            .collect();
        Ok(LogCarry { slots, pending })
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use crate::checkpoint::log_carry::LogCarry;

    impl LogCarry {
        /// A carry of no slot and no input, for a test of a checkpoint outside a log.
        pub const fn empty() -> LogCarry {
            LogCarry {
                slots: Vec::new(),
                pending: Vec::new(),
            }
        }
    }
}
