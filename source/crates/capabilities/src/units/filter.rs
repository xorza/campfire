use crate::scripts::error::ApiError;
use crate::units::tag_set::TagSet;
use crate::units::unit_types::UnitTypes;
use crate::values::attitude::Attitude;
use crate::values::filter_data::{FilterData, FilterSyntax};
use crate::values::relation::Relation;

/// A filter as a match runs it: a relation, the tags a unit must have, and those it must not,
/// among them the tags of delivery units, `projectile` and `area`, but one it must have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Filter {
    relation: Relation,
    all: TagSet,
    none: TagSet,
}

impl Filter {
    /// The filter `text` names, with its tags among the match's.
    pub(crate) fn parse(text: &str, types: &UnitTypes) -> Result<Filter, ApiError> {
        let syntax = FilterSyntax::parse(text).ok_or(ApiError::UnknownFilter)?;
        let terms = syntax.tags().map(|term| (term.name, term.negated));
        Filter::of(syntax.relation, terms, types)
    }

    /// The run-time form of `data`, with its tags among the match's.
    pub(crate) fn resolve(data: &FilterData, types: &UnitTypes) -> Result<Filter, ApiError> {
        let terms = data.tags.iter().map(|tag| (tag.name.as_str(), tag.negated));
        Filter::of(data.relation, terms, types)
    }

    fn of<'a>(
        relation: Relation,
        terms: impl Iterator<Item = (&'a str, bool)>,
        types: &UnitTypes,
    ) -> Result<Filter, ApiError> {
        let mut filter = Filter {
            relation,
            all: TagSet::default(),
            none: TagSet::default(),
        };
        for (name, negated) in terms {
            let tag = types.tag(name).ok_or(ApiError::UnknownTag)?;
            if negated {
                filter.none = filter.none.with(tag);
            } else {
                filter.all = filter.all.with(tag);
            }
        }
        for tag in types.deliveries().iter() {
            if !filter.all.contains(tag) {
                filter.none = filter.none.with(tag);
            }
        }
        Ok(filter)
    }

    /// Whether it selects a unit with `tags` of a team regarded with `attitude`, as the unit it
    /// selects for regards it.
    pub(crate) const fn selects(self, attitude: Attitude, tags: TagSet) -> bool {
        self.relation.selects(attitude) && tags.covers(self.all) && !tags.meets(self.none)
    }
}
