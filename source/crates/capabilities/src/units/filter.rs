use crate::combat::team::Team;
use crate::units::error::ApiError;
use crate::units::filter_data::FilterData;
use crate::units::relation::Relation;
use crate::units::tag_set::{Tag, TagSet};
use crate::units::unit_types::UnitTypes;

/// A filter as a match runs it: a relation, and the tag of a unit type it may need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Filter {
    relation: Relation,
    tag: Option<Tag>,
}

/// A filter as data and scripts write it: a relation and an optional tag after a colon, such as
/// `enemies:creep`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FilterSyntax<'a> {
    pub(crate) relation: Relation,
    pub(crate) tag: Option<&'a str>,
}

impl Filter {
    /// The filter `text` names, with its tag among those of the match's unit types.
    pub(crate) fn parse(text: &str, types: &UnitTypes) -> Result<Filter, ApiError> {
        let syntax = FilterSyntax::parse(text).ok_or(ApiError::UnknownFilter)?;
        let tag = syntax
            .tag
            .map(|name| types.tag(name).ok_or(ApiError::UnknownTag))
            .transpose()?;
        Ok(Filter {
            relation: syntax.relation,
            tag,
        })
    }

    /// The run-time form of `data`, with its tag among those of the match's unit types.
    pub(crate) fn resolve(data: &FilterData, types: &UnitTypes) -> Result<Filter, ApiError> {
        let tag = data.tag.as_deref();
        let tag = tag.map(|name| types.tag(name).ok_or(ApiError::UnknownTag));
        Ok(Filter {
            relation: data.relation,
            tag: tag.transpose()?,
        })
    }

    /// Whether it selects a unit of `team` with `tags`, relative to a unit of `of`.
    pub(crate) const fn selects(self, of: Team, team: Team, tags: TagSet) -> bool {
        let tagged = match self.tag {
            Some(tag) => tags.contains(tag),
            None => true,
        };
        tagged && self.relation.holds(of, team)
    }
}

impl<'a> FilterSyntax<'a> {
    /// `text` as a filter; `None` unless its relation is `enemies`, `allies` or `all`.
    pub(crate) fn parse(text: &'a str) -> Option<FilterSyntax<'a>> {
        let (relation, tag) = match text.split_once(':') {
            Some((relation, tag)) => (relation, Some(tag)),
            None => (text, None),
        };
        Some(FilterSyntax {
            relation: Relation::parse(relation)?,
            tag,
        })
    }
}
