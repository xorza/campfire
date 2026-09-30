use crate::combat::team::Team;
use crate::units::error::ApiError;
use crate::units::relation::Relation;
use crate::units::tag_set::{Tag, TagSet};
use crate::units::unit_types::UnitTypes;

/// A filter as scripts write it: a relation and an optional tag after a colon, such as
/// `enemies:creep`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Filter {
    relation: Relation,
    tag: Option<Tag>,
}

impl Filter {
    /// The filter `text` names, with its tag among those of the match's unit types.
    pub(crate) fn parse(text: &str, types: &UnitTypes) -> Result<Filter, ApiError> {
        let (relation, tag) = match text.split_once(':') {
            Some((relation, tag)) => (relation, Some(tag)),
            None => (text, None),
        };
        let relation = Relation::parse(relation).ok_or(ApiError::UnknownFilter)?;
        let tag = tag
            .map(|name| types.tag(name).ok_or(ApiError::UnknownTag))
            .transpose()?;
        Ok(Filter { relation, tag })
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
