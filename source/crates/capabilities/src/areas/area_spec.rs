use campfire_math::Num;
use campfire_sim::Ticks;

use crate::stats::modifier_book::ModifierId;
use crate::units::filter::Filter;

/// An area type as a match runs it: its radius, its delay and its duration in ticks, what it
/// reaches, and the modifiers it holds inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AreaSpec {
    pub(crate) radius: Num,
    pub(crate) delay: Ticks,
    pub(crate) duration: Ticks,
    pub(crate) affects: Filter,
    pub(crate) inside: Inside,
}

/// The modifiers an area holds on its source, on the source's other allies, and on the units
/// that may be attacked, while they are inside.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Inside {
    pub(crate) caster: Option<ModifierId>,
    pub(crate) allies: Option<ModifierId>,
    pub(crate) enemies: Option<ModifierId>,
}
