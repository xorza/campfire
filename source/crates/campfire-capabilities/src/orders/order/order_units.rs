use std::slice;

use campfire_sim::StableId;
use serde::{Serialize, Serializer};

/// The units an order goes to: at least one, in increasing stable id, each once. One unit is held
/// in place, so an order to one unit allocates nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderUnits(Units);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Units {
    One(StableId),
    Many(Box<[StableId]>),
}

impl OrderUnits {
    pub const fn one(unit: StableId) -> OrderUnits {
        OrderUnits(Units::One(unit))
    }

    /// The list `units`; `None` unless it has one at least and rises strictly.
    pub fn new(units: &[StableId]) -> Option<OrderUnits> {
        match units {
            [] => None,
            &[unit] => Some(OrderUnits::one(unit)),
            _ => OrderUnits::rises(units).then(|| OrderUnits(Units::Many(units.into()))),
        }
    }

    /// Whether each of `units` comes after the one before it.
    pub(crate) fn rises(units: &[StableId]) -> bool {
        units.windows(2).all(|pair| pair[0] < pair[1])
    }

    pub fn get(&self) -> &[StableId] {
        match &self.0 {
            Units::One(unit) => slice::from_ref(unit),
            Units::Many(units) => units,
        }
    }
}

/// A list of stable ids, its length first, as the decode of an order reads it.
impl Serialize for OrderUnits {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.get().serialize(serializer)
    }
}
