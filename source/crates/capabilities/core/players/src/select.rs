use campfire_codec::{Binary, Decoded};
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

use crate::order_units::OrderUnits;

/// A select, the body of a command to the players: the units its player's next orders command,
/// in increasing stable id, each once (04-capabilities/control.md, A select holds what its player
/// controls).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Select<'a> {
    pub selecting: Selecting,
    pub units: &'a [StableId],
}

/// What a select's units do to those its player commands: take their place, or join them, as a
/// select past one payload continues in the next input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Selecting {
    New,
    More,
}

impl Select<'_> {
    /// Appends its body to `out`: its selecting, then its units as a list.
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        debug_assert!(OrderUnits::rises(self.units), "a select's units rise");
        Binary::encode_into(&(self.selecting, self.units), out);
    }

    /// The selecting of the select `body` holds, its units appended to `units`; `None`, with
    /// `units` as it was, unless the body is exactly one select of at least one unit, in
    /// increasing stable id, each once: a client can send any bytes.
    pub fn decode_into(body: &[u8], units: &mut Vec<StableId>) -> Option<Selecting> {
        let start = units.len();
        let selecting = Binary::take::<Selecting>(body).ok().and_then(|selecting| {
            let rest = read_units(selecting.rest, units)?;
            rest.is_empty().then_some(selecting.value)
        });
        if selecting.is_none() {
            units.truncate(start);
        }
        selecting
    }
}

/// Appends the units `body` lists first to `units`, and gives the bytes after them; `None` unless
/// they are one at least, in increasing stable id, each once.
fn read_units<'b>(body: &'b [u8], units: &mut Vec<StableId>) -> Option<&'b [u8]> {
    let Decoded {
        value: count,
        mut rest,
    } = Binary::take::<usize>(body).ok()?;
    // Each id takes a byte at least, so a count past the bytes left is a flaw, found before it
    // reserves anything.
    if count == 0 || count > rest.len() {
        return None;
    }
    let start = units.len();
    units.reserve(count);
    for _ in 0..count {
        let unit = Binary::take::<StableId>(rest).ok()?;
        units.push(unit.value);
        rest = unit.rest;
    }
    OrderUnits::rises(&units[start..]).then_some(rest)
}
