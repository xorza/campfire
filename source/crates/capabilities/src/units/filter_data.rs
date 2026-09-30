use std::fmt;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::units::filter::FilterSyntax;
use crate::units::relation::Relation;

/// A filter in data, such as an area's `affects`: a relation, and a tag after a colon that the
/// load checks against the mode's unit types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterData {
    pub relation: Relation,
    pub tag: Option<String>,
}

impl FilterData {
    /// `text` as a filter; `None` unless its relation is `enemies`, `allies` or `all`.
    pub fn parse(text: &str) -> Option<FilterData> {
        let syntax = FilterSyntax::parse(text)?;
        Some(FilterData {
            relation: syntax.relation,
            tag: syntax.tag.map(str::to_owned),
        })
    }
}

impl<'de> Deserialize<'de> for FilterData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<FilterData, D::Error> {
        let text = String::deserialize(deserializer)?;
        FilterData::parse(&text)
            .ok_or_else(|| D::Error::custom(format!("filter {text:?}: no relation")))
    }
}

/// As data writes it.
impl fmt::Display for FilterData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let relation = self.relation.name();
        match &self.tag {
            Some(tag) => write!(f, "{relation}:{tag}"),
            None => f.write_str(relation),
        }
    }
}
