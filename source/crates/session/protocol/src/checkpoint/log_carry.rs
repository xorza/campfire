use campfire_common::{Seat, Tick};
use serde::{Deserialize, Serialize};

use crate::bytes::Bytes;
use crate::checkpoint::error::CheckpointDecodeError;
use crate::control::Control;
use crate::delegation::Delegation;
use crate::main_key::MainKey;
use crate::spill::Spill;
use crate::stamp_count::StampCount;

/// The log's own state at a checkpoint's boundary, which a segment's verification from its
/// checkpoint alone, planned, will start from: each seat's controller, with a player's delegation
/// and chain, the main key of the player who left it last, its stamp counts and its spill; and the
/// inputs logged before the boundary and due from it on, in the order they apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogCarry {
    pub(crate) seats: Vec<CarriedSeat>,
    pub(crate) pending: Vec<CarriedInput>,
}

/// One seat of a `LogCarry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarriedSeat {
    pub(crate) control: Control<Box<Delegation>>,
    pub(crate) leaver: Option<MainKey>,
    pub(crate) stamps: StampCount,
    pub(crate) spill: Spill,
}

/// An input logged before the boundary that applies in `tick`, from it on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarriedInput {
    pub(crate) tick: Tick,
    pub(crate) seat: Seat,
    pub(crate) stamp: Tick,
    pub(crate) payload: Vec<u8>,
}

/// A `LogCarry` as it is signed and logged: a delegation as its JSON.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CarryWire<'a> {
    #[serde(borrow)]
    seats: Vec<SeatWire<'a>>,
    #[serde(borrow)]
    pending: Vec<InputWire<'a>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SeatWire<'a> {
    #[serde(borrow)]
    control: Control<&'a str>,
    leaver: Option<MainKey>,
    stamps: StampCount,
    spill: Spill,
}

#[derive(Debug, Serialize, Deserialize)]
struct InputWire<'a> {
    tick: Tick,
    seat: Seat,
    stamp: Tick,
    #[serde(borrow)]
    payload: Bytes<'a>,
}

impl LogCarry {
    pub(crate) fn wire(&self) -> CarryWire<'_> {
        let seats = self
            .seats
            .iter()
            .map(|seat| SeatWire {
                control: seat.control.as_ref().map(|delegation| delegation.json()),
                leaver: seat.leaver,
                stamps: seat.stamps,
                spill: seat.spill,
            })
            .collect();
        let pending = self
            .pending
            .iter()
            .map(|input| InputWire {
                tick: input.tick,
                seat: input.seat,
                stamp: input.stamp,
                payload: Bytes(&input.payload),
            })
            .collect();
        CarryWire { seats, pending }
    }

    /// The carry `wire` holds; an error for a delegation that does not parse.
    pub(crate) fn from_wire(wire: CarryWire<'_>) -> Result<Self, CheckpointDecodeError> {
        let mut seats = Vec::with_capacity(wire.seats.len());
        for (seat, carried) in (0..).zip(wire.seats) {
            let control = carried.control.try_map(|delegation| {
                Delegation::parse(delegation)
                    .map(Box::new)
                    .map_err(|error| CheckpointDecodeError::Delegation {
                        seat: Seat::new(seat),
                        error,
                    })
            })?;
            seats.push(CarriedSeat {
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
                seat: input.seat,
                stamp: input.stamp,
                payload: input.payload.0.to_vec(),
            })
            .collect();
        Ok(Self { seats, pending })
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::checkpoint::log_carry::LogCarry;

    impl LogCarry {
        /// A carry of no seat and no input, for a test of a checkpoint outside a log.
        pub const fn empty() -> Self {
            Self {
                seats: Vec::new(),
                pending: Vec::new(),
            }
        }
    }
}
