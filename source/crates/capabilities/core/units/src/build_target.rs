use campfire_math::Num;
use serde::{Deserialize, Serialize};

use crate::order_refs::{Ids, OrderRefs, RefMap, SerdeRefs};

/// Where a build order places its building: at a point of the ground plane, its box turned
/// `angle` degrees, or at a site of the same building to build on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum BuildTarget<R: OrderRefs = Ids> {
    Point { at: R::Place, angle: Num },
    Site(R::Unit),
}

impl Copy for BuildTarget {}

impl<R: OrderRefs> BuildTarget<R> {
    /// The same target with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<BuildTarget<T>, M::Error> {
        Ok(match self {
            Self::Point { at, angle } => BuildTarget::Point {
                at: refs.place(at, angle)?,
                angle,
            },
            Self::Site(site) => BuildTarget::Site(refs.unit(site)?),
        })
    }
}
