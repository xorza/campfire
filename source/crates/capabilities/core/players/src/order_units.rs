use std::slice;

use campfire_sim::StableId;
use serde::{Serialize, Serializer};

/// The units an order goes to: at least one, in increasing stable id, each once, or none for an
/// order its player carries out, as a perk's learn, which goes with no select. One unit is held in
/// place, so an order to one unit allocates nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderUnits(Units);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Units {
    None,
    One(StableId),
    Many(Box<[StableId]>),
}

impl OrderUnits {
    /// No unit: the units of an order its player carries out.
    pub const NONE: Self = Self(Units::None);

    pub const fn one(id: StableId) -> Self {
        Self(Units::One(id))
    }

    /// The list `units`; `None` unless it has one at least and rises strictly.
    pub fn new(units: &[StableId]) -> Option<Self> {
        match units {
            [] => None,
            &[unit] => Some(Self::one(unit)),
            _ => Self::rises(units).then(|| Self(Units::Many(units.into()))),
        }
    }

    /// Whether each of `units` comes after the one before it.
    pub(crate) fn rises(units: &[StableId]) -> bool {
        units.windows(2).all(|pair| pair[0] < pair[1])
    }

    pub fn get(&self) -> &[StableId] {
        match &self.0 {
            Units::None => &[],
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
