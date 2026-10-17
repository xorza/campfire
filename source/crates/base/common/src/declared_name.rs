use std::borrow::Borrow;
use std::fmt;

use derive_more::Display;
use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// A name a mode declares in its data and scripts use, such as a damage kind, a stat, a resource
/// or a map: a lowercase letter, then lowercase letters, digits and underscores, at most
/// `MAX_LEN` bytes. Every name of that grammar is one, so the grammar has one home.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct DeclaredName(Box<str>);

impl DeclaredName {
    /// The longest name: a few words joined by underscores, as a person types a key of a mode's
    /// data, so a name an untrusted message gives costs at most that.
    pub const MAX_LEN: usize = 64;

    /// `text` as a name; `None` unless it is of that form.
    pub fn new(text: &str) -> Option<Self> {
        Self::admits(text).then(|| Self(text.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `text` is of the form of a name.
    fn admits(text: &str) -> bool {
        let rest = |byte: &u8| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_';
        match text.as_bytes() {
            [first, rest_of @ ..] => {
                text.len() <= Self::MAX_LEN
                    && first.is_ascii_lowercase()
                    && rest_of.iter().all(rest)
            }
            [] => false,
        }
    }
}

impl PartialEq<str> for DeclaredName {
    fn eq(&self, other: &str) -> bool {
        *self.0 == *other
    }
}

/// By its text, so a map keyed by names finds a name a script or a filter gives.
impl Borrow<str> for DeclaredName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// From the text a format gives, checked before the one allocation of the name.
impl<'de> Deserialize<'de> for DeclaredName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(NameVisitor)
    }
}

/// Reads a `DeclaredName` from a string.
#[derive(Debug)]
struct NameVisitor;

impl Visitor<'_> for NameVisitor {
    type Value = DeclaredName;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a name: a lowercase letter, then lowercase letters, digits and underscores, at most \
             {} bytes",
            DeclaredName::MAX_LEN
        )
    }

    fn visit_str<E: Error>(self, text: &str) -> Result<DeclaredName, E> {
        DeclaredName::new(text).ok_or_else(|| {
            E::custom(format!(
                "{text:?} is not a name: a lowercase letter, then lowercase letters, digits and \
                 underscores, at most {} bytes",
                DeclaredName::MAX_LEN
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    #[test]
    fn a_name_starts_with_a_letter_holds_letters_digits_and_underscores_and_has_a_bound() {
        let longest = "a".repeat(DeclaredName::MAX_LEN);
        for good in ["magic", "armor_pen_pct", "x2", "a", &longest] {
            assert_eq!(
                DeclaredName::new(good).map(|name| name.to_string()),
                Some(good.to_owned())
            );
        }
        let past = "a".repeat(DeclaredName::MAX_LEN + 1);
        for bad in [
            "",
            "2x",
            "_x",
            "Magic",
            "fire damage",
            "armor-pen",
            "é",
            &past,
        ] {
            assert_eq!(DeclaredName::new(bad), None, "{bad:?}");
        }
        // Postcard writes a name as its length and its bytes, and a decode checks it.
        let magic = DeclaredName::new("magic").unwrap();
        let encoded = Binary::encode(&magic);
        assert_eq!(encoded, b"\x05magic");
        assert_eq!(Binary::decode::<DeclaredName>(&encoded).unwrap(), magic);
        assert!(Binary::decode::<DeclaredName>(b"\x05Magic").is_err());
    }
}
