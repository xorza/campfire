use std::borrow::Borrow;

use derive_more::Display;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A package's name, such as `hero-husk`: a lowercase letter, then lowercase letters, digits,
/// hyphens and underscores. It is one component of a locale file's path, and an avatar's names
/// the avatar's unit type, so it holds no separator and no character a script's name lacks but
/// the hyphen.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageName(Box<str>);

impl PackageName {
    /// `text` as a name; `None` unless it is of that form.
    pub fn new(text: &str) -> Option<PackageName> {
        let mut chars = text.chars();
        let first = chars.next()?;
        let rest = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
        (first.is_ascii_lowercase() && chars.all(rest)).then(|| PackageName(text.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for PackageName {
    fn eq(&self, other: &str) -> bool {
        *self.0 == *other
    }
}

impl PartialEq<&str> for PackageName {
    fn eq(&self, other: &&str) -> bool {
        *self.0 == **other
    }
}

/// By its text, so a map keyed by package names finds a name a manifest gives.
impl Borrow<str> for PackageName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for PackageName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<PackageName, D::Error> {
        let text = String::deserialize(deserializer)?;
        PackageName::new(&text).ok_or_else(|| {
            D::Error::custom(format!(
                "{text:?} is not a package name: a lowercase letter, then lowercase letters, \
                 digits, hyphens and underscores"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::Toml;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct Header {
        name: PackageName,
    }

    #[test]
    fn a_package_name_is_one_path_component_of_lowercase_words() {
        for good in ["hero-husk", "moba-3v3", "player_spells", "x"] {
            assert_eq!(
                PackageName::new(good).as_ref().map(PackageName::as_str),
                Some(good)
            );
        }
        for bad in [
            "",
            "Hero",
            "3v3",
            "-x",
            "hero/husk",
            "hero husk",
            "..",
            "héro",
            "a.b",
        ] {
            assert_eq!(PackageName::new(bad), None, "{bad:?}");
        }
        // A manifest's name reads only in that form.
        let read = |text: &str| Toml::parse::<Header>(text).map(|header| header.name);
        assert_eq!(read(r#"name = "hero-gale""#).unwrap(), "hero-gale");
        assert!(read(r#"name = "Hero/Gale""#).is_err());
    }
}
