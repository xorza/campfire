use campfire_math::Num;
use campfire_sim::{TickRate, Ticks};

use crate::areas::area_data::AreaData;
use crate::stats::modifier_book::ModifierId;
use crate::units::filter::Filter;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;
use crate::values::relation::Relation;

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

impl AreaSpec {
    /// The spec of `data`, which the package load checked: its times in ticks at `rate`, rounded
    /// up, what it reaches among the tags of `types`, and its `inside` modifiers as `modifier`
    /// finds them; `None` when a time does not count in ticks.
    pub(crate) fn of(
        data: &AreaData,
        types: &UnitTypes,
        rate: TickRate,
        modifier: impl Fn(&DeclaredName) -> ModifierId,
    ) -> Option<AreaSpec> {
        let affects = match &data.affects {
            Some(filter) => Filter::resolve(filter, types),
            None => Ok(Filter::of_relation(Relation::Enemies)),
        };
        let inside = |name: &Option<DeclaredName>| name.as_ref().map(&modifier);
        Some(AreaSpec {
            radius: data.radius,
            delay: rate.ticks(data.delay_ms)?,
            duration: rate.ticks(data.duration_ms)?,
            affects: affects.expect("the load checked the filter's tags"),
            inside: Inside {
                caster: inside(&data.inside.caster),
                allies: inside(&data.inside.allies),
                enemies: inside(&data.inside.enemies),
            },
        })
    }
}
