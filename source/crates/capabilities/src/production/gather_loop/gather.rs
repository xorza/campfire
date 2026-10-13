use campfire_math::Num;

use crate::actions::gather_spec::GatherSpec;
use crate::units::filter::Filter;

/// A worker's gather: its action's spec, its filter of nodes, its range, and its time in ticks.
#[derive(Debug, Clone, Copy)]
pub(super) struct Gather {
    pub(super) spec: GatherSpec,
    pub(super) filter: Filter,
    pub(super) range: Num,
    pub(super) time: u64,
}
