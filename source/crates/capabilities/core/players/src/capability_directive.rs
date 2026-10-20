use campfire_codec::{Binary, Taken};
use campfire_sim::{Capability, StableId};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::order_units::OrderUnits;

/// The directive of one capability's orders, the body of each command to that capability after the
/// units it names.
pub trait CapabilityDirective: Serialize + DeserializeOwned + Copy + Send + Sync + 'static {
    /// The capability whose commands hold it.
    const CAPABILITY: Capability;

    /// Appends the units of the order `body` holds to `units`, and gives its directive; `None`,
    /// with `units` as it was, unless the body is exactly one order, to at least one unit, in
    /// increasing stable id, each once: a client can send any bytes.
    fn decode_into(body: &[u8], units: &mut Vec<StableId>) -> Option<Self> {
        let start = units.len();
        let directive = read_units(body, units).and_then(|rest| {
            let Taken { value, rest } = Binary::take::<Self>(rest).ok()?;
            rest.is_empty().then_some(value)
        });
        if directive.is_none() {
            units.truncate(start);
        }
        directive
    }
}

/// Appends the units `body` lists first to `units`, and gives the bytes after them; `None` unless
/// they are one at least, in increasing stable id, each once.
fn read_units<'b>(body: &'b [u8], units: &mut Vec<StableId>) -> Option<&'b [u8]> {
    let Taken {
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
