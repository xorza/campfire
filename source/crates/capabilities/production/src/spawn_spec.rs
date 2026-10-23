use std::num::NonZeroU8;

use campfire_common::Ticks;
use campfire_declarations::PerId;
use campfire_values::UnitType;

/// A unit type's spawns as a match runs them: `count` units of `unit_types`, taken in turn, each
/// that dies coming back `replace` ticks later, rounded up (04-capabilities/production.md, A
/// building may keep spawns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnSpec {
    pub unit_types: Box<[UnitType]>,
    pub count: NonZeroU8,
    pub replace: Ticks,
}

impl SpawnSpec {
    /// The most unit types it takes in turn, as a spawner's turn is a `u8`.
    pub const MOST_TYPES: usize = 256;

    /// The unit type of the spawn at `turn`, and the turn after it.
    pub(crate) fn turn(&self, turn: u8) -> SpawnTurn {
        let place = usize::from(turn);
        let next = (place + 1) % self.unit_types.len();
        SpawnTurn {
            unit_type: self.unit_types[place],
            next: u8::try_from(next).expect("a spawner takes at most 256 types"),
        }
    }
}

/// The unit type of a spawn at its spawner's turn, and the turn after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpawnTurn {
    pub(crate) unit_type: UnitType,
    pub(crate) next: u8,
}

impl PerId<UnitType> for SpawnSpec {
    const SCRIPTS_ONLY: bool = false;
}
