use std::str::FromStr;

use derive_more::Display;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::version::error::NotVersion;

pub(crate) mod error;

/// A version as `major.minor.patch`, each a decimal with no leading zero but a lone one, as
/// Semantic Versioning has it, so one version has one spelling: a package's, the engine release
/// a package targets, and the engine release a session runs on, whose terms hash its text.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[display("{major}.{minor}.{patch}")]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    /// This build's engine release: the workspace's version.
    pub const ENGINE: Version = match Version::parse(env!("CARGO_PKG_VERSION")) {
        Some(version) => version,
        None => panic!("the workspace's version is a version"),
    };

    pub const fn new(major: u32, minor: u32, patch: u32) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    /// `text` as a version; `None` unless it is three decimals joined by dots, each with no
    /// leading zero but a lone one.
    pub const fn parse(text: &str) -> Option<Version> {
        let bytes = text.as_bytes();
        let mut parts = [0_u32; 3];
        let mut part = 0;
        let mut digits = 0;
        let mut at = 0;
        while at < bytes.len() {
            match bytes[at] {
                b'.' if digits > 0 && part < 2 => {
                    part += 1;
                    digits = 0;
                }
                byte @ b'0'..=b'9' => {
                    if digits == 1 && parts[part] == 0 {
                        return None;
                    }
                    let Some(shifted) = parts[part].checked_mul(10) else {
                        return None;
                    };
                    let Some(value) = shifted.checked_add((byte - b'0') as u32) else {
                        return None;
                    };
                    parts[part] = value;
                    digits += 1;
                }
                _ => return None,
            }
            at += 1;
        }
        if part != 2 || digits == 0 {
            return None;
        }
        Some(Version::new(parts[0], parts[1], parts[2]))
    }
}

impl FromStr for Version {
    type Err = NotVersion;

    fn from_str(text: &str) -> Result<Version, NotVersion> {
        Version::parse(text).ok_or(NotVersion)
    }
}

/// As its text, which a log's header, an offer and a package's manifest carry.
impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// A log, a message or a manifest is untrusted, so a text `parse` refuses fails to decode.
impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Version, D::Error> {
        let text = String::deserialize(deserializer)?;
        Version::parse(&text)
            .ok_or_else(|| D::Error::custom(format!("{text:?} is not major.minor.patch")))
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    #[test]
    fn a_version_reads_only_in_its_one_spelling_and_writes_it_back() {
        for (text, version) in [
            ("0.1.0", Version::new(0, 1, 0)),
            ("12.340.5", Version::new(12, 340, 5)),
            ("10.0.100", Version::new(10, 0, 100)),
            ("4294967295.0.0", Version::new(u32::MAX, 0, 0)),
        ] {
            assert_eq!(text.parse(), Ok(version), "{text}");
            assert_eq!(version.to_string(), text);
        }
        // Too few or too many parts, an empty one, a leading zero, a sign, a letter or a space,
        // a pre-release, and a part past a `u32`.
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
            "1.0.007",
            "+1.2.3",
            "-1.0.0",
            "1.2.x",
            "1.2.3 ",
            "1.2.3-rc.1",
            "4294967296.0.0",
        ] {
            assert_eq!(text.parse::<Version>(), Err(NotVersion), "{text}");
        }
        assert!(Version::new(0, 10, 0) > Version::new(0, 9, 9));
        assert_eq!(Version::ENGINE.to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn a_version_encodes_as_its_text_and_a_text_of_another_form_does_not_decode() {
        let version = Version::new(1, 20, 3);
        let bytes = Binary::encode(&version);
        assert_eq!(bytes, Binary::encode("1.20.3"));
        assert_eq!(Binary::decode::<Version>(&bytes).ok(), Some(version));
        let other = Binary::encode("1.020.3");
        assert!(Binary::decode::<Version>(&other).is_err());
    }
}
