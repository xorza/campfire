use campfire_common::DeclaredName;
use campfire_units::OrderRefs;
use campfire_values::FilterData;

use crate::place::Place;

/// The references of an order as a bot's script writes it: a unit by a filter, of which the
/// nearest unit it selects is the one; a slot by its action's name; a place by a map marker's
/// name or its point; an item type by its name (04-capabilities/control.md, A bot's script
/// names what it orders).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Named;

impl OrderRefs for Named {
    type Unit = FilterData;
    type Slot = DeclaredName;
    type Ground = Place;
    type Point = Place;
    type Item = DeclaredName;
}
