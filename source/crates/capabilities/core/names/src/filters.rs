use campfire_values::{FilterData, Relation, TagSet};

use crate::error::UnknownName;
use crate::filter::Filter;
use crate::tag_names::TagNames;

/// Filters as a match runs them, which select a unit when any one of them does, as 0 A.D.'s class
/// lists and OpenRA's `ValidTargets` select by any one term; none of them selects no unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters(Box<[Filter]>);

impl Filters {
    /// The run-time form of each of `filters`, data's, as `Filter::resolve` gives it.
    pub fn resolve(filters: &[FilterData], tags: &TagNames) -> Result<Self, UnknownName> {
        filters
            .iter()
            .map(|filter| Filter::resolve(filter, tags))
            .collect()
    }

    /// Whether any of them selects a unit with `tags` of a team regarded with `relation`.
    pub fn selects(&self, relation: Relation, tags: TagSet) -> bool {
        self.0.iter().any(|filter| filter.selects(relation, tags))
    }

    /// Whether it holds none, so it selects no unit.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Filter> {
        self.0.iter()
    }
}

impl FromIterator<Filter> for Filters {
    fn from_iter<I: IntoIterator<Item = Filter>>(filters: I) -> Self {
        Self(filters.into_iter().collect())
    }
}

impl<const N: usize> From<[Filter; N]> for Filters {
    fn from(filters: [Filter; N]) -> Self {
        filters.into_iter().collect()
    }
}

impl<'a> IntoIterator for &'a Filters {
    type Item = &'a Filter;
    type IntoIter = std::slice::Iter<'a, Filter>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
