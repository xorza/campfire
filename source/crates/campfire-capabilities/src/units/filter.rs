use crate::scripts::error::ApiError;
use crate::units::engine_tag::EngineTag;
use crate::units::tag_set::TagSet;
use crate::units::unit_types::UnitTypes;
use crate::values::filter_data::{FilterData, FilterSyntax};
use crate::values::relation::Relation;
use crate::values::relation_set::RelationSet;

/// The engine tags of delivery units, which a filter leaves out unless it names one.
const DELIVERY: [EngineTag; 2] = [EngineTag::Projectile, EngineTag::Area];

/// A filter as a match runs it: a set of relations, the tags a unit must have, and those it must
/// not, among them the tags of delivery units, `projectile` and `area`, but one it must have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Filter {
    relations: RelationSet,
    all: TagSet,
    none: TagSet,
}

impl Filter {
    /// The filter `text` names, with its tags among the match's.
    pub(crate) fn parse(text: &str, types: &UnitTypes) -> Result<Filter, ApiError> {
        let syntax = FilterSyntax::parse(text).ok_or(ApiError::UnknownFilter)?;
        let terms = syntax.tags().map(|term| (term.name, term.negated));
        Filter::of(syntax.relations, terms, types)
    }

    /// The run-time form of `data`, with its tags among the match's.
    pub(crate) fn resolve(data: &FilterData, types: &UnitTypes) -> Result<Filter, ApiError> {
        let terms = data.tags.iter().map(|tag| (tag.name.as_str(), tag.negated));
        Filter::of(data.relations, terms, types)
    }

    /// The run-time form of `data`, as `resolve` gives it, or with none, the filter of enemies
    /// alone.
    pub(crate) fn resolve_or_enemies(
        data: Option<&FilterData>,
        types: &UnitTypes,
    ) -> Result<Filter, ApiError> {
        data.map_or(Ok(Filter::of_relations(RelationSet::Enemies)), |data| {
            Filter::resolve(data, types)
        })
    }

    /// The filter of `relations` alone: every unit it selects, but delivery units.
    pub(crate) fn of_relations(relations: RelationSet) -> Filter {
        Filter {
            relations,
            all: TagSet::default(),
            none: TagSet::of(DELIVERY.map(EngineTag::tag)),
        }
    }

    fn of<'a>(
        relations: RelationSet,
        terms: impl Iterator<Item = (&'a str, bool)>,
        types: &UnitTypes,
    ) -> Result<Filter, ApiError> {
        let mut filter = Filter {
            relations,
            all: TagSet::default(),
            none: TagSet::default(),
        };
        for (name, negated) in terms {
            let tag = types.tag_named(name).ok_or(ApiError::UnknownTag)?;
            if negated {
                filter.none = filter.none.with(tag);
            } else {
                filter.all = filter.all.with(tag);
            }
        }
        for tag in DELIVERY.map(EngineTag::tag) {
            if !filter.all.contains(tag) {
                filter.none = filter.none.with(tag);
            }
        }
        Ok(filter)
    }

    /// Whether it selects a unit with `tags` of a team regarded with `relation`, as the unit it
    /// selects for regards it.
    pub(crate) const fn selects(self, relation: Relation, tags: TagSet) -> bool {
        self.relations.selects(relation) && tags.covers(self.all) && !tags.meets(self.none)
    }
}
