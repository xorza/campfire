use campfire_common::DeclaredName;
use campfire_script::{DataSection, DataTable};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A train's or a build's `requires`: unit types of the action's package, for each entry of which
/// its player must own a living, complete unit of one of its types, and player modifiers of the
/// package it must hold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiresData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<RequiredUnit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<DeclaredName>,
}

/// An entry of a `requires`' `units`: a unit type's name, or a list of names any one of which
/// holds it, as Zero Hour's prerequisite line of several objects (12-base-defenses.md, A
/// requirement may be any one of several types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredUnit(Vec<DeclaredName>);

impl DataSection for RequiresData {
    const TABLE: DataTable = DataTable::new("A train's `requires`");
}

impl RequiredUnit {
    /// An entry any one of `names` holds; `None` for none.
    pub fn any_of(names: impl IntoIterator<Item = DeclaredName>) -> Option<Self> {
        let names: Vec<DeclaredName> = names.into_iter().collect();
        (!names.is_empty()).then_some(Self(names))
    }

    /// The names any one of which holds it, one at least.
    pub fn names(&self) -> &[DeclaredName] {
        &self.0
    }
}

impl From<DeclaredName> for RequiredUnit {
    fn from(name: DeclaredName) -> Self {
        Self(vec![name])
    }
}

/// Data is untrusted, so an empty list fails to read.
impl<'de> Deserialize<'de> for RequiredUnit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Entry {
            One(DeclaredName),
            Any(Vec<DeclaredName>),
        }
        match Entry::deserialize(deserializer)? {
            Entry::One(name) => Ok(Self::from(name)),
            Entry::Any(names) => Self::any_of(names).ok_or_else(|| {
                D::Error::custom("a list of required unit types holds one at least")
            }),
        }
    }
}

/// As data writes it: one name alone, several as a list.
impl Serialize for RequiredUnit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0.as_slice() {
            [name] => name.serialize(serializer),
            names => names.serialize(serializer),
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn an_entry_reads_a_name_or_a_list_and_writes_one_name_alone() {
        let read = |text: &str| Toml::parse::<RequiresData>(text);
        let name = |text: &str| DeclaredName::new(text).unwrap();
        let data = read("units = [\"barracks\", [\"factory\", \"airfield\"]]").unwrap();
        let any = RequiredUnit::any_of([name("factory"), name("airfield")]).unwrap();
        assert_eq!(data.units, [RequiredUnit::from(name("barracks")), any]);
        assert_eq!(read(&Toml::write(&data)).unwrap(), data);
        // A list of one writes as its name; an empty list does not read.
        let single = read("units = [[\"barracks\"]]").unwrap();
        assert_eq!(
            Toml::write(&single),
            Toml::write(&read("units = [\"barracks\"]").unwrap())
        );
        let message = read("units = [[]]").unwrap_err().message().to_owned();
        assert!(message.contains("holds one at least"), "{message}");
    }
}
