use std::fmt;
use std::path::{Component, Path};

use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A path to a file inside a package: relative, of plain names only, so it cannot leave the
/// package. Data names its scripts with one, so a path that leaves is refused where the data is
/// read.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackagePath(String);

impl PackagePath {
    /// `text` as a path in a package; `None` when it leaves the package: empty, absolute, or
    /// through `..`.
    pub fn parse(text: &str) -> Option<PackagePath> {
        let mut components = Path::new(text).components().peekable();
        if components.peek().is_none()
            || !components.all(|component| matches!(component, Component::Normal(_)))
        {
            return None;
        }
        Some(PackagePath(text.to_owned()))
    }

    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl fmt::Display for PackagePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PackagePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<PackagePath, D::Error> {
        let text = String::deserialize(deserializer)?;
        PackagePath::parse(&text)
            .ok_or_else(|| D::Error::custom(format!("{text:?}: outside the package")))
    }
}
