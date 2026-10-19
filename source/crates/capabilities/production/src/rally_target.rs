use campfire_units::{Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

/// Where a producer sends the units it trains: a point of the ground plane, or a unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum RallyTarget<R: OrderRefs = Ids> {
    Point(R::Ground),
    Unit(R::Unit),
}

impl Copy for RallyTarget {}

impl<R: OrderRefs> RallyTarget<R> {
    /// The same target with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<RallyTarget<T>, M::Error> {
        Ok(match self {
            Self::Point(at) => RallyTarget::Point(refs.ground(at)?),
            Self::Unit(unit) => RallyTarget::Unit(refs.unit(unit)?),
        })
    }
}
