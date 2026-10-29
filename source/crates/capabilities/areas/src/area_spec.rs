use campfire_common::{DeclaredName, Ticks};
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_math::Num;
use campfire_script::{Filter, TagNames, UnknownName};
use campfire_sim::MatchRate;
use campfire_stats::{Affects, ModifierBook};
use campfire_units::UnitTypes;
use campfire_values::{ModifierId, Named, RelationSet, TimeTooLarge, UnitType};

use crate::area_data::AreaData;
use crate::error::AreaDataError;

/// An area type as a match runs it: its radius, its delay and its duration in ticks, what it
/// reaches, and the modifiers it holds inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AreaSpec {
    pub radius: Num,
    pub delay: Ticks,
    pub duration: Ticks,
    pub affects: Filter,
    pub inside: Inside,
}

/// The modifiers an area holds on the units inside it, each with the units it affects: its
/// source, the source's other allies, or the other units that may be attacked, as the filter of
/// those relations alone selects them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Inside([Option<(Affects, ModifierId)>; 3]);

impl AreaSpec {
    /// The spec of `data`: its times in ticks at `rate`, rounded up, what it reaches among the
    /// tags of `tags`, and its `inside` modifiers as `modifier` finds them; an error for a time
    /// that does not count in ticks, or a name the match does not have.
    pub fn of(
        data: &AreaData,
        tags: &TagNames,
        rate: MatchRate,
        modifier: impl Fn(&DeclaredName) -> Result<ModifierId, UnknownName>,
    ) -> Result<Self, AreaDataError> {
        let inside = |affects, name: &Option<DeclaredName>| {
            let modifier = name.as_ref().map(&modifier).transpose()?;
            Ok::<_, UnknownName>(modifier.map(|modifier| (affects, modifier)))
        };
        let others = |relations| Affects::Others(Filter::of_relations(relations, tags));
        let ticks = |ms| rate.ticks(ms).ok_or(TimeTooLarge);
        Ok(Self {
            radius: data.radius,
            delay: ticks(data.delay_ms)?,
            duration: ticks(data.duration_ms)?,
            affects: Filter::resolve_or_enemies(data.affects.as_ref(), tags)?,
            inside: Inside([
                inside(Affects::Source, &data.inside.caster)?,
                inside(others(RelationSet::Allies), &data.inside.allies)?,
                inside(others(RelationSet::Enemies), &data.inside.enemies)?,
            ]),
        })
    }
}

impl Inside {
    /// Each modifier it holds, with the units it affects.
    pub(crate) fn holds(self) -> impl Iterator<Item = (Affects, ModifierId)> {
        self.0.into_iter().flatten()
    }

    /// Each modifier it holds.
    pub(crate) fn modifiers(self) -> impl Iterator<Item = ModifierId> {
        self.holds().map(|(_, modifier)| modifier)
    }
}

impl PerId<UnitType> for AreaSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl AreaSpec {
    /// The spec of each area type, its filters resolved against the match's tags and its modifiers
    /// among its package's; an error for a name the match does not have, or a time too large to
    /// count in ticks.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let tags = cx.fact::<UnitTypes>()?.tags();
        let modifiers = cx.book::<ModifierBook>()?;
        let mut book = ByIdBuilder::default();
        for unit in cx.types() {
            let Some(area) = unit.data.section::<AreaData>() else {
                continue;
            };
            let modifier = |name: &DeclaredName| {
                let modifier = modifiers.names().named((unit.package, name.as_str()));
                modifier.ok_or_else(|| UnknownName::new(ModifierId::NAMES, name))
            };
            let at = || BookPlace::UnitType(unit.place());
            let spec = Self::of(area, tags, cx.rate(), modifier).map_err(|error| match error {
                AreaDataError::Time(error) => BookError::rule(unit.package, at(), error),
                AreaDataError::Unknown(name) => BookError::unknown(unit.package, at(), name),
            })?;
            book.set(unit.unit_type, spec);
        }
        Ok(book.build())
    }
}
