use std::str::FromStr;

use derive_more::Display;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::map_name::error::NotMapName;

pub(crate) mod error;

/// The name of one of a mode's maps, as its directory `map/<name>/` holds it and a session's
/// terms name it: a declared name, a lowercase letter, then lowercase letters, digits and
/// underscores, as a mode's data names what it declares.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct MapName(Box<str>);

impl MapName {
    /// `text` as a map's name; `None` unless it is of that form.
    pub fn new(text: &str) -> Option<MapName> {
        let mut chars = text.chars();
        let first = chars.next()?;
        let rest = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_';
        (first.is_ascii_lowercase() && chars.all(rest)).then(|| MapName(text.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for MapName {
    type Err = NotMapName;

    fn from_str(text: &str) -> Result<MapName, NotMapName> {
        MapName::new(text).ok_or(NotMapName)
    }
}

/// A log or a message is untrusted, so a name `new` refuses fails to decode.
impl<'de> Deserialize<'de> for MapName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<MapName, D::Error> {
        let text = String::deserialize(deserializer)?;
        MapName::new(&text).ok_or_else(|| D::Error::custom(NotMapName))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_name_is_a_declared_name_and_encodes_as_its_text() {
        for good in ["lane", "two_lanes", "t3", "a"] {
            assert_eq!(good.parse::<MapName>().unwrap().as_str(), good);
        }
        for bad in ["", "Lane", "3v3", "_a", "two-lanes", "a b", "é"] {
            assert_eq!(bad.parse::<MapName>(), Err(NotMapName), "{bad:?}");
        }
        // Postcard writes a string as its length and its bytes.
        let lane = MapName::new("lane").unwrap();
        let encoded = postcard::to_allocvec(&lane).unwrap();
        assert_eq!(encoded, b"\x04lane");
        assert_eq!(postcard::from_bytes::<MapName>(&encoded).unwrap(), lane);
        assert!(postcard::from_bytes::<MapName>(b"\x04Lane").is_err());
    }
}
