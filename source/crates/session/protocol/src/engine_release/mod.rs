use std::fmt;
use std::str::FromStr;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::engine_release::error::NotRelease;

pub(crate) mod error;

/// The release of the engine a session runs on, as its terms name it: `major.minor.patch`, each
/// a decimal with no leading zero, so one release has one spelling, and the session id one hash of
/// it. A session plays, restores and verifies on its own release alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EngineRelease {
    major: u32,
    minor: u32,
    patch: u32,
}

impl EngineRelease {
    /// This build's release: the workspace's version.
    pub const CURRENT: EngineRelease = match EngineRelease::parse(env!("CARGO_PKG_VERSION")) {
        Some(release) => release,
        None => panic!("the workspace's version is an engine release"),
    };

    pub const fn new(major: u32, minor: u32, patch: u32) -> EngineRelease {
        EngineRelease {
            major,
            minor,
            patch,
        }
    }

    pub const fn is_current(self) -> bool {
        let current = EngineRelease::CURRENT;
        self.major == current.major && self.minor == current.minor && self.patch == current.patch
    }

    /// `text` as a release; `None` unless it is of that form.
    const fn parse(text: &str) -> Option<EngineRelease> {
        let bytes = text.as_bytes();
        let mut parts = [0_u32; 3];
        let (mut part, mut at, mut digits) = (0, 0, 0);
        while at < bytes.len() {
            let byte = bytes[at];
            at += 1;
            if byte == b'.' {
                if digits == 0 || part == 2 {
                    return None;
                }
                (part, digits) = (part + 1, 0);
                continue;
            }
            if !byte.is_ascii_digit() || (digits == 1 && parts[part] == 0) {
                return None;
            }
            let Some(value) = parts[part].checked_mul(10) else {
                return None;
            };
            let Some(value) = value.checked_add((byte - b'0') as u32) else {
                return None;
            };
            (parts[part], digits) = (value, digits + 1);
        }
        if digits == 0 || part != 2 {
            return None;
        }
        Some(EngineRelease::new(parts[0], parts[1], parts[2]))
    }
}

impl fmt::Display for EngineRelease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for EngineRelease {
    type Err = NotRelease;

    fn from_str(text: &str) -> Result<EngineRelease, NotRelease> {
        EngineRelease::parse(text).ok_or(NotRelease)
    }
}

/// As its text, which the log's header and the offer carry.
impl Serialize for EngineRelease {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// A log or a message is untrusted, so a text `from_str` refuses fails to decode.
impl<'de> Deserialize<'de> for EngineRelease {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<EngineRelease, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    #[test]
    fn a_release_reads_only_in_its_one_spelling_and_writes_it_back() {
        for (text, release) in [
            ("0.1.0", EngineRelease::new(0, 1, 0)),
            ("12.0.340", EngineRelease::new(12, 0, 340)),
            ("4294967295.0.0", EngineRelease::new(u32::MAX, 0, 0)),
        ] {
            assert_eq!(text.parse(), Ok(release), "{text}");
            assert_eq!(release.to_string(), text);
        }
        // Too few or too many parts, an empty one, a leading zero, a sign or a space, a
        // pre-release, and a part past a `u32`.
        for text in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "1..3",
            ".2.3",
            "1.2.",
            "01.2.3",
            "1.02.3",
            "1.2.00",
            "+1.2.3",
            "1.2.3 ",
            "1.2.3-rc.1",
            "4294967296.0.0",
        ] {
            assert_eq!(text.parse::<EngineRelease>(), Err(NotRelease), "{text}");
        }
        assert_eq!(
            EngineRelease::CURRENT.to_string(),
            env!("CARGO_PKG_VERSION")
        );
        assert!(EngineRelease::CURRENT.is_current());
        assert!(!EngineRelease::new(0, 0, 9).is_current());
    }

    #[test]
    fn a_release_encodes_as_its_text_and_a_text_of_another_form_does_not_decode() {
        let release = EngineRelease::new(1, 20, 3);
        let bytes = Binary::encode(&release);
        assert_eq!(bytes, Binary::encode("1.20.3"));
        assert_eq!(Binary::decode::<EngineRelease>(&bytes).ok(), Some(release));
        let other = Binary::encode("1.020.3");
        assert!(Binary::decode::<EngineRelease>(&other).is_err());
    }
}
