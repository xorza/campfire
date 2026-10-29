use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::zero_hour::road_drop_cause::RoadDropCause;

/// A map's road segments of a type, by the type's name or their template, that are not drawn,
/// why, and how many.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedRoads {
    pub road: String,
    pub cause: RoadDropCause,
    pub count: usize,
}

/// The road's name, its cause, then its count.
impl Display for DroppedRoads {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({:?}), {}", self.road, self.cause, self.count)
    }
}
