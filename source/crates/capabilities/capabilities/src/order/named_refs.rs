use campfire_common::DeclaredName;
use campfire_units::OrderRefs;
use campfire_values::FilterData;

use crate::order::location::{GroundLocation, Location};

/// The references of an order as a bot's script writes it: a unit by a filter, of which the
/// nearest unit it selects is the one; a slot by its action's name; a location by a map marker's
/// name or its point, and a build's place by one or by a search round one; an item type, and a stack
/// a unit carries, by the item type's name; a perk by its name (04-capabilities/control.md, A bot's
/// script names what it orders).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedRefs;

impl OrderRefs for NamedRefs {
    type Unit = FilterData;
    type Slot = DeclaredName;
    type Ground = Location;
    type Place = GroundLocation;
    type Point = Location;
    type Item = DeclaredName;
    type Carried = DeclaredName;
    type Perk = DeclaredName;
}
