use std::str::FromStr;

use derive_more::Display;
use serde::{Deserialize, Serialize};

use crate::declared_name::DeclaredName;
use crate::map_name::error::NotMapName;

pub(crate) mod error;

/// The name of one of a mode's maps, as its directory `map/<name>/` holds it and a session's
/// terms name it: a declared name, as a mode's data names what it declares.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MapName(DeclaredName);

impl MapName {
    /// `text` as a map's name; `None` unless it is a declared name.
    pub fn parse(text: &str) -> Option<Self> {
        DeclaredName::new(text).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl FromStr for MapName {
    type Err = NotMapName;

    fn from_str(text: &str) -> Result<Self, NotMapName> {
        Self::parse(text).ok_or(NotMapName)
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

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
        let lane = MapName::parse("lane").unwrap();
        let encoded = Binary::encode(&lane);
        assert_eq!(encoded, b"\x04lane");
        assert_eq!(Binary::decode::<MapName>(&encoded).unwrap(), lane);
        assert!(Binary::decode::<MapName>(b"\x04Lane").is_err());
    }
}
