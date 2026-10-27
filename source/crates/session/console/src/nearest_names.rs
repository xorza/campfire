use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The names nearest one a game lacks, by edit distance, the nearest first, names of one distance
/// in their order, so a caller corrects a name it misspelled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct NearestNames(Vec<String>);

impl NearestNames {
    /// The most names a refusal gives.
    pub const MOST: usize = 5;

    /// The names of `names` nearest `name`, by Levenshtein's distance, at most `MOST`.
    pub fn of<'a>(name: &str, names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut far: Vec<(usize, &str)> = names
            .into_iter()
            .map(|other| (strsim::levenshtein(name, other), other))
            .collect();
        far.sort_unstable();
        far.dedup_by_key(|&mut (_, other)| other);
        Self(
            far.into_iter()
                .take(Self::MOST)
                .map(|(_, other)| other.to_owned())
                .collect(),
        )
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

impl<'a> IntoIterator for &'a NearestNames {
    type Item = &'a String;
    type IntoIter = std::slice::Iter<'a, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_names_come_first_and_at_most_five() {
        let names = ["creep", "tower", "hero", "creeps", "crept", "tree", "cree"];
        // "creep" is 0 away; "cree" and "creeps" 1; "crept" and "tree" 2; "hero" and "tower" more.
        let nearest = NearestNames::of("creep", names);
        assert_eq!(
            nearest.iter().collect::<Vec<_>>(),
            ["creep", "cree", "creeps", "crept", "tree"]
        );
        // "towr" is 1 from "tower" and more from "hero"; a name twice comes once.
        let nearest = NearestNames::of("towr", ["tower", "hero", "tower"]);
        assert_eq!(nearest.iter().collect::<Vec<_>>(), ["tower", "hero"]);
        assert_eq!(NearestNames::of("x", []).iter().len(), 0);
    }
}
