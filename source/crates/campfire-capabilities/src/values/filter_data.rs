use std::fmt;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::declared_name::DeclaredName;
use crate::values::relation_set::RelationSet;

/// A filter in data, such as an area's `affects`: a set of relations, then tags after colons,
/// each one the unit must have, or with `!` one it must not, which the load checks against the
/// mode's tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterData {
    pub relations: RelationSet,
    pub tags: Vec<FilterTag>,
}

/// A tag of a filter: one the unit must have, or, negated, one it must not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterTag {
    pub name: DeclaredName,
    pub negated: bool,
}

/// A filter as data and scripts write it: a set of relations, then tags after colons, such as
/// `enemies:avatar:!stunned`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FilterSyntax<'a> {
    pub(crate) relations: RelationSet,
    tags: &'a str,
}

/// A tag as a filter writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TagTerm<'a> {
    pub(crate) name: &'a str,
    pub(crate) negated: bool,
}

impl FilterData {
    /// `text` as a filter; `None` unless its set of relations is one of the sets and each of its
    /// tags is a name.
    pub fn parse(text: &str) -> Option<FilterData> {
        let syntax = FilterSyntax::parse(text)?;
        let tags = syntax.tags().map(|term| {
            Some(FilterTag {
                name: DeclaredName::new(term.name)?,
                negated: term.negated,
            })
        });
        Some(FilterData {
            relations: syntax.relations,
            tags: tags.collect::<Option<_>>()?,
        })
    }
}

impl<'de> Deserialize<'de> for FilterData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<FilterData, D::Error> {
        let text = String::deserialize(deserializer)?;
        FilterData::parse(&text).ok_or_else(|| D::Error::custom(format!("filter {text:?}")))
    }
}

/// As data writes it.
impl fmt::Display for FilterData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.relations.name())?;
        for tag in &self.tags {
            let sign = if tag.negated { "!" } else { "" };
            write!(f, ":{sign}{}", tag.name)?;
        }
        Ok(())
    }
}

impl<'a> FilterSyntax<'a> {
    /// `text` as a filter; `None` unless its set of relations is one of the sets and no tag of it
    /// is empty.
    pub(crate) fn parse(text: &'a str) -> Option<FilterSyntax<'a>> {
        let (relations, tags, tagged) = match text.split_once(':') {
            Some((relations, tags)) => (relations, tags, true),
            None => (text, "", false),
        };
        let syntax = FilterSyntax {
            relations: RelationSet::named(relations)?,
            tags,
        };
        let complete =
            !tagged || (!tags.is_empty() && syntax.tags().all(|term| !term.name.is_empty()));
        complete.then_some(syntax)
    }

    /// Its tags, in the order it writes them.
    pub(crate) fn tags(self) -> impl Iterator<Item = TagTerm<'a>> {
        let tags = (!self.tags.is_empty()).then_some(self.tags);
        tags.into_iter()
            .flat_map(|tags| tags.split(':'))
            .map(|term| match term.strip_prefix('!') {
                Some(name) => TagTerm {
                    name,
                    negated: true,
                },
                None => TagTerm {
                    name: term,
                    negated: false,
                },
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_reads_its_relation_and_its_signed_tags() {
        let data = FilterData::parse("enemies:avatar:!stunned").unwrap();
        assert_eq!(data.relations, RelationSet::Enemies);
        let tags: Vec<_> = data
            .tags
            .iter()
            .map(|tag| (tag.name.as_str(), tag.negated))
            .collect();
        assert_eq!(tags, [("avatar", false), ("stunned", true)]);
        assert_eq!(data.to_string(), "enemies:avatar:!stunned");
        assert_eq!(FilterData::parse("all").unwrap().tags, []);
        for text in ["friends", "enemies:", "enemies::creep", "enemies:!", ""] {
            assert_eq!(FilterData::parse(text), None, "{text}");
        }
    }
}
