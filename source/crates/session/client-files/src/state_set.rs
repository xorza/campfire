use campfire_common::DeclaredName;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A set of conditions a state is drawn for: the names of the conditions of a test it names, and
/// how many conditions of its game it also names that no test says, which no unit holds, so each
/// counts as one it lacks (02-engine-core.md, A set counts the conditions no test says). A units
/// file writes a set of none of them as its list of names, `["damaged", "night"]`, and one of some
/// as `{ names = ["damaged"], untested = 1 }`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateSet {
    names: Vec<DeclaredName>,
    untested: u32,
}

impl StateSet {
    /// The set of `names` and of `untested` conditions no test says.
    pub fn new(names: impl IntoIterator<Item = DeclaredName>, untested: u32) -> Self {
        Self {
            names: names.into_iter().collect(),
            untested,
        }
    }

    pub fn names(&self) -> &[DeclaredName] {
        &self.names
    }

    pub const fn untested(&self) -> u32 {
        self.untested
    }

    /// Whether a unit draws this set over `other` whatever it holds, this set coming first when
    /// `first`, by `SetMatch::best`'s rule: this set names each of `other`'s names, so it holds
    /// as many in common at least, and its other names, which a unit may lack, and its untested
    /// conditions are fewer than `other`'s untested conditions, or as many with this set first,
    /// so as it holds no more in common it lacks fewer.
    pub fn beats(&self, other: &Self, first: bool) -> bool {
        if !other.names.iter().all(|name| self.names.contains(name)) {
            return false;
        }
        let others = self.names.iter().filter(|name| !other.names.contains(name));
        let others = u32::try_from(others.count()).expect("a set's names count in a u32");
        let lacked = others + self.untested;
        lacked < other.untested || (lacked == other.untested && first)
    }
}

/// The set of `names`, of no untested condition.
impl FromIterator<DeclaredName> for StateSet {
    fn from_iter<I: IntoIterator<Item = DeclaredName>>(names: I) -> Self {
        Self::new(names, 0)
    }
}

impl<const N: usize> From<[DeclaredName; N]> for StateSet {
    fn from(names: [DeclaredName; N]) -> Self {
        names.into_iter().collect()
    }
}

/// A set's form in a units file: its list of names, of no untested condition, or a table of its
/// names and its count of untested conditions.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SetForm {
    Names(Vec<DeclaredName>),
    Counted(CountedSet),
}

/// A set's table: its names and its count of untested conditions.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CountedSet<N = Vec<DeclaredName>> {
    names: N,
    untested: u32,
}

impl Serialize for StateSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.untested == 0 {
            return self.names.serialize(serializer);
        }
        let counted = CountedSet {
            names: &self.names,
            untested: self.untested,
        };
        counted.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StateSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match SetForm::deserialize(deserializer)? {
            SetForm::Names(names) => Self::new(names, 0),
            SetForm::Counted(CountedSet { names, untested }) => Self::new(names, untested),
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    fn set(names: &[&str], untested: u32) -> StateSet {
        StateSet::new(
            names.iter().map(|name| DeclaredName::new(name).unwrap()),
            untested,
        )
    }

    #[test]
    fn a_set_beats_another_it_holds_whatever_its_unit_holds_when_it_lacks_fewer() {
        // `damaged` and 1 untested beats `damaged` and 2, before it or after: as many in common
        // whatever the unit holds, and one fewer lacked; `damaged, night` and 0 beats it only first,
        // as a unit off a night lacks `night` as many as the 1 untested; `night` and 0 holds no
        // `damaged`, so a damaged unit draws the other.
        let other = set(&["damaged"], 2);
        assert!(set(&["damaged"], 1).beats(&other, false));
        assert!(set(&["damaged", "night"], 1).beats(&other, true));
        assert!(!set(&["damaged", "night"], 1).beats(&other, false));
        assert!(!set(&["night"], 0).beats(&other, true));
        // A set of no untested condition is beaten only by its equal before it.
        let tested = set(&["damaged"], 0);
        assert!(tested.beats(&tested, true));
        assert!(!tested.beats(&tested, false));
        assert!(!set(&["damaged", "night"], 0).beats(&tested, true));
    }

    #[test]
    fn a_set_writes_as_its_names_or_as_a_table_of_its_count() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Sets {
            sets: Vec<StateSet>,
        }
        let sets = Sets {
            sets: vec![set(&["damaged"], 0), set(&["damaged"], 2)],
        };
        let written = Toml::write(&sets);
        assert_eq!(
            written,
            "sets = [[\"damaged\"], { names = [\"damaged\"], untested = 2 }]\n"
        );
        assert_eq!(Toml::parse::<Sets>(&written).unwrap(), sets);
        // A table of a field it does not know is refused.
        let extra = "sets = [{ names = [], untested = 1, held = 2 }]";
        assert!(Toml::parse::<Sets>(extra).is_err());
    }
}
