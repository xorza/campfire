use campfire_math::Num;
use campfire_values::FilterData;
use serde::Serialize;

/// An area type's section as the import writes it: its radius, and the units it reaches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AreaWrite {
    pub(crate) radius: Num,
    pub(crate) affects: FilterData,
}
