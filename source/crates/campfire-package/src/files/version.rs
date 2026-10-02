use std::fmt;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A version as `major.minor.patch`, each a decimal number: a package's version, or the engine
/// release a package targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// `text` as a version; `None` unless it is three decimal numbers joined by dots, with no
    /// leading zero but a lone one, as Semantic Versioning has it, so each version has one
    /// spelling.
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
        Some(Version {
            major: parts[0],
            minor: parts[1],
            patch: parts[2],
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Version, D::Error> {
        let text = String::deserialize(deserializer)?;
        Version::parse(&text)
            .ok_or_else(|| D::Error::custom(format!("{text:?} is not major.minor.patch")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_three_decimal_numbers() {
        let version = |major, minor, patch| {
            Some(Version {
                major,
                minor,
                patch,
            })
        };
        assert_eq!(Version::parse("0.1.0"), version(0, 1, 0));
        assert_eq!(Version::parse("12.340.5"), version(12, 340, 5));
        assert_eq!(Version::parse("10.0.100"), version(10, 0, 100));
        assert_eq!(Version::parse("4294967295.0.0"), version(u32::MAX, 0, 0));
        for text in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "1..3",
            ".1.2",
            "1.2.",
            "1.2.x",
            "4294967296.0.0",
            "-1.0.0",
            "01.0.0",
            "1.00.0",
            "1.0.007",
        ] {
            assert_eq!(Version::parse(text), None, "{text:?}");
        }
        assert_eq!(Version::parse("0.1.0").unwrap().to_string(), "0.1.0");
        assert!(Version::parse("0.10.0") > Version::parse("0.9.9"));
    }
}
