use std::fmt::Debug;

use campfire_math::Vec3;
use campfire_sim::StableId;
use campfire_values::ItemId;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::ground_point::GroundPoint;

/// What an order's directive names a unit, a slot, a place and an item type by: its ids, `Ids`, as
/// a command carries them, or another form while the order is not due, as a bot's script names
/// them (04-capabilities/control.md, A directive is generic over its references).
pub trait OrderRefs: Debug + Clone + Copy + PartialEq + Eq {
    type Unit: Debug + Clone + PartialEq + Eq;
    type Slot: Debug + Clone + PartialEq + Eq;
    /// A point of the ground plane.
    type Ground: Debug + Clone + PartialEq + Eq;
    /// A point of the map, a height among its coordinates.
    type Point: Debug + Clone + PartialEq + Eq;
    type Item: Debug + Clone + PartialEq + Eq;
}

/// References that serde writes and reads, which each action's serde form names as its bound.
/// The bound serde infers, `R::Unit: Deserialize<'de>`, holds a lifetime, which panics rustc
/// 1.100.0-beta.5 as it checks an action that must be `DeserializeOwned`; this one holds none.
pub trait SerdeRefs:
    OrderRefs<
        Unit: Serialize + DeserializeOwned,
        Slot: Serialize + DeserializeOwned,
        Ground: Serialize + DeserializeOwned,
        Point: Serialize + DeserializeOwned,
        Item: Serialize + DeserializeOwned,
    >
{
}

impl<R> SerdeRefs for R where
    R: OrderRefs<
            Unit: Serialize + DeserializeOwned,
            Slot: Serialize + DeserializeOwned,
            Ground: Serialize + DeserializeOwned,
            Point: Serialize + DeserializeOwned,
            Item: Serialize + DeserializeOwned,
        >
{
}

/// The references of an order as a command carries it: a unit by its stable id, a slot by its
/// place among the unit's, a place by its coordinates, and an item type by its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ids;

impl OrderRefs for Ids {
    type Unit = StableId;
    type Slot = u8;
    type Ground = GroundPoint;
    type Point = Vec3;
    type Item = ItemId;
}

/// Gives the references of `To` for those of `From`, each in its turn as an action's map of
/// references reaches it, or the error that stops the map.
pub trait RefMap<From: OrderRefs, To: OrderRefs> {
    type Error;

    fn unit(&mut self, unit: From::Unit) -> Result<To::Unit, Self::Error>;

    fn slot(&mut self, slot: From::Slot) -> Result<To::Slot, Self::Error>;

    fn ground(&mut self, ground: From::Ground) -> Result<To::Ground, Self::Error>;

    fn point(&mut self, point: From::Point) -> Result<To::Point, Self::Error>;

    fn item(&mut self, item: From::Item) -> Result<To::Item, Self::Error>;
}
