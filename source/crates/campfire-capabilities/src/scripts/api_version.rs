use std::fmt;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A version of the package API, the script API and the schemas of the data files together, as
/// `major.minor`: a name or a field the API adds raises the minor, and one it removes or changes
/// raises the major.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApiVersion {
    pub major: u16,
    pub minor: u16,
}

impl ApiVersion {
    /// The first version, which every name of the release came in.
    pub const FIRST: ApiVersion = ApiVersion { major: 1, minor: 0 };
    /// The version the release runs.
    pub const RELEASE: ApiVersion = ApiVersion::FIRST;

    /// `text` as a version; `None` unless it is two decimal numbers joined by a dot, with no
    /// leading zero but a lone one.
    pub fn parse(text: &str) -> Option<ApiVersion> {
        let (major, minor) = text.split_once('.')?;
        let number = |part: &str| {
            let plain = part.bytes().all(|byte| byte.is_ascii_digit())
                && !part.is_empty()
                && (part == "0" || !part.starts_with('0'));
            plain.then(|| part.parse::<u16>().ok()).flatten()
        };
        Some(ApiVersion {
            major: number(major)?,
            minor: number(minor)?,
        })
    }

    /// Whether a release of this version loads a package that targets `package`: of the same
    /// major, and a minor no higher, as every name and field it names is then one this release
    /// has, with the same meaning.
    pub const fn loads(self, package: ApiVersion) -> bool {
        package.major == self.major && package.minor <= self.minor
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl<'de> Deserialize<'de> for ApiVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ApiVersion, D::Error> {
        let text = String::deserialize(deserializer)?;
        ApiVersion::parse(&text)
            .ok_or_else(|| D::Error::custom(format!("{text:?} is not major.minor")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_loads_its_major_up_to_its_minor() {
        let version = |major, minor| ApiVersion { major, minor };
        assert_eq!(ApiVersion::parse("1.0"), Some(version(1, 0)));
        assert_eq!(ApiVersion::parse("12.30"), Some(version(12, 30)));
        for text in [
            "1", "1.0.0", "01.0", "1.00", "1.", ".1", "a.b", "1.-1", "65536.0", "",
        ] {
            assert_eq!(ApiVersion::parse(text), None, "{text:?}");
        }
        // A release of 2.3 loads 2.0 to 2.3, and no other.
        let release = version(2, 3);
        let loads = [(2, 0), (2, 2), (2, 3), (2, 4), (1, 9), (3, 0), (3, 3)]
            .map(|(major, minor)| release.loads(version(major, minor)));
        assert_eq!(loads, [true, true, true, false, false, false, false]);
        assert_eq!(version(2, 3).to_string(), "2.3");
    }
}
