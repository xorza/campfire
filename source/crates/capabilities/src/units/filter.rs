use crate::scripts::error::ApiError;
use crate::units::tag_set::{Tag, TagSet};
use crate::units::team::Team;
use crate::units::unit_types::UnitTypes;
use crate::values::filter_data::{FilterData, FilterSyntax};
use crate::values::relation::Relation;

/// A filter as a match runs it: a relation, and the tag of a unit type it may need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Filter {
    relation: Relation,
    tag: Option<Tag>,
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
        let related = match self.relation {
            Relation::Enemies => of.is_enemy_of(team),
            Relation::Allies => !of.is_enemy_of(team),
            Relation::All => true,
        };
        tagged && related
    }
}
