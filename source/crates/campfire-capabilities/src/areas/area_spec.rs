use campfire_common::Ticks;
use campfire_math::Num;
use campfire_sim::TickRate;

use crate::areas::area_data::AreaData;
use crate::areas::error::AreaError;
use crate::declare::per_type::PerType;
use crate::scripts::error::UnknownName;
use crate::units::filter::Filter;
use crate::units::modifier_id::ModifierId;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;
use crate::values::error::TimeTooLarge;
use campfire_sim::Capability;

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
    /// The spec of `data`: its times in ticks at `rate`, rounded up, what it reaches among the
    /// tags of `types`, and its `inside` modifiers as `modifier` finds them; an error for a time
    /// that does not count in ticks, or a name the match does not have.
    pub(crate) fn of(
        data: &AreaData,
        types: &UnitTypes,
        rate: TickRate,
        modifier: impl Fn(&DeclaredName) -> Result<ModifierId, UnknownName>,
    ) -> Result<AreaSpec, AreaError> {
        let inside = |name: &Option<DeclaredName>| name.as_ref().map(&modifier).transpose();
        let ticks = |ms| rate.ticks(ms).ok_or(TimeTooLarge);
        Ok(AreaSpec {
            radius: data.radius,
            delay: ticks(data.delay_ms)?,
            duration: ticks(data.duration_ms)?,
            affects: Filter::resolve_or_enemies(data.affects.as_ref(), types)?,
            inside: Inside {
                caster: inside(&data.inside.caster)?,
                allies: inside(&data.inside.allies)?,
                enemies: inside(&data.inside.enemies)?,
            },
        })
    }
}

impl Inside {
    /// Each modifier it holds, on the source, its allies and the units that may be attacked.
    pub(crate) fn modifiers(self) -> impl Iterator<Item = ModifierId> {
        [self.caster, self.allies, self.enemies]
            .into_iter()
            .flatten()
    }
}

impl PerType for AreaSpec {
    const OWNER: Option<Capability> = Some(Capability::Areas);
    const SCRIPTS_ONLY: bool = false;
}
