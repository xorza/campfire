use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::filter_data::{FilterData, FilterTag};
use crate::relation_set::RelationSet;

/// A filter in data that selects by tags alone, its relations `all`, as a death's `victim`
/// filter, which no relation of the unit it selects for holds: a mode's bounty and its share of
/// experience select their victims by it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagFilterData(FilterData);

impl TagFilterData {
    /// The filter of `tags`, its relations `all`, as a tool writes one.
    pub fn new(tags: impl IntoIterator<Item = FilterTag>) -> Self {
        Self(FilterData {
            relations: RelationSet::All,
            tags: tags.into_iter().collect(),
        })
    }

    /// The filter, its relations `all`.
    pub const fn filter(&self) -> &FilterData {
        &self.0
    }
}

/// Every unit, as a mode that names no victim filter selects.
impl Default for TagFilterData {
    fn default() -> Self {
        Self(FilterData::all())
    }
}

/// Data is untrusted, so a filter of relations other than `all` fails to read.
impl<'de> Deserialize<'de> for TagFilterData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let filter = FilterData::deserialize(deserializer)?;
        if filter.relations != RelationSet::All {
            return Err(D::Error::custom(format!(
                "filter {filter} selects by tags alone, its relations all"
            )));
        }
        Ok(Self(filter))
    }
}

/// As data writes it, the text `Deserialize` reads.
impl Serialize for TagFilterData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;
    use campfire_common::DeclaredName;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct Holder {
        victim: TagFilterData,
    }

    #[test]
    fn a_tag_filter_reads_tags_alone() {
        let read =
            |text: &str| Toml::parse::<Holder>(&format!("victim = {text:?}")).map(|h| h.victim);
        let filter = read("all:!constructing").unwrap();
        assert_eq!(filter.filter().to_string(), "all:!constructing");
        assert_eq!(
            read("enemies:!constructing").unwrap_err().message(),
            "filter enemies:!constructing selects by tags alone, its relations all"
        );
        assert_eq!(TagFilterData::default().filter(), &FilterData::all());
        // A filter made of its tags writes as one read of them.
        let tag = |name: &str, negated| FilterTag {
            name: DeclaredName::new(name).unwrap(),
            negated,
        };
        let made = TagFilterData::new([tag("constructing", true), tag("armored", false)]);
        assert_eq!(made, read("all:!constructing:armored").unwrap());
    }
}
