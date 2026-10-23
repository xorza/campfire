use campfire_common::{DeclaredName, MapName};

use crate::zero_hour::error::NameError;
use crate::zero_hour::format::ini::object_ini::IniObject;

/// A name of the original, a map's directory or an object's template, as a package names it: its
/// ASCII letters lowercase, its digits and `_` kept, each other byte `_`, and `x` before a name
/// that does not start with a letter. Two names may give one, which the import refuses.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ImportName(String);

impl ImportName {
    pub(crate) fn of(original: &[u8]) -> Self {
        let mut name: String = original
            .iter()
            .map(|&byte| match byte.to_ascii_lowercase() {
                kept @ (b'a'..=b'z' | b'0'..=b'9') => char::from(kept),
                _ => '_',
            })
            .collect();
        if !name.starts_with(|first: char| first.is_ascii_lowercase()) {
            name.insert(0, 'x');
        }
        Self(name)
    }

    /// The name a package gives `object`, of its name's bytes, each char of Latin-1 one, as a
    /// map's template writes it.
    pub(crate) fn object(object: &IniObject) -> Self {
        Self::latin(&object.name)
    }

    /// The name a package gives `name`, an INI's text, each char of Latin-1 one byte.
    pub(crate) fn latin(name: &str) -> Self {
        let bytes: Vec<u8> = name
            .chars()
            .map(|char| u8::try_from(char).expect("an INI's text is Latin-1"))
            .collect();
        Self::of(&bytes)
    }

    /// The declared name it is; an error past `DeclaredName::MAX_LEN` bytes, the one rule of a
    /// declared name its letters can break.
    pub(crate) fn declared(&self) -> Result<DeclaredName, NameError> {
        DeclaredName::new(&self.0).ok_or_else(|| NameError::TooLong(self.0.clone()))
    }

    /// The map name it is, as `declared` checks it.
    pub(crate) fn map(&self) -> Result<MapName, NameError> {
        MapName::parse(&self.0).ok_or_else(|| NameError::TooLong(self.0.clone()))
    }

    /// The id of `kind` of it, `<kind>_<name>`, as an action or a gather names its type.
    pub(crate) fn id(&self, kind: &str) -> Result<DeclaredName, NameError> {
        Self(format!("{kind}_{}", self.0)).declared()
    }

    /// The name of it and `suffix`, `<name>_<suffix>`, as a stage names its action's.
    pub(crate) fn suffixed(&self, suffix: &str) -> Result<DeclaredName, NameError> {
        Self(format!("{}_{suffix}", self.0)).declared()
    }
}

#[cfg(test)]
pub(crate) mod reach_in {
    use crate::zero_hour::import_name::ImportName;

    impl ImportName {
        pub(crate) fn as_str(&self) -> &str {
            &self.0
        }
    }
}

#[cfg(test)]
mod tests;
