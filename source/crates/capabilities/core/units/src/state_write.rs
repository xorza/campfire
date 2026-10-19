use campfire_script::{StateField, StatePlace, StateValue};
use campfire_sim::StableId;

/// A call's write of `value` to `field` of unit `id`, held at `place`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StateWrite {
    pub id: StableId,
    pub(crate) field: StateField,
    pub(crate) place: StatePlace,
    pub(crate) value: StateValue,
}
