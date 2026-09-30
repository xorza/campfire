use std::fmt;
use std::path::{Component, Path};

use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::error::ContentError;

/// A path to a file inside a package: relative, of plain names only, so it cannot leave the
/// package. Data names its scripts with one, so a path that leaves is refused where the data is
/// read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackagePath(String);

impl PackagePath {
    pub fn parse(text: &str) -> Result<PackagePath, ContentError> {
        let mut components = Path::new(text).components().peekable();
        if components.peek().is_none()
            || !components.all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(ContentError::OutsidePackage(text.to_owned()));
        }
        Ok(PackagePath(text.to_owned()))
    }

    pub(crate) fn as_path(&self) -> &Path {
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
        PackagePath::parse(&text).map_err(D::Error::custom)
    }
}
