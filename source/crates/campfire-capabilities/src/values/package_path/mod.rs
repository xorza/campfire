use derive_more::Display;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A path to a file inside a package: names joined by `/`, each one not empty, not `.` or `..`,
/// and holding no `\`, so it cannot leave the package, reads the same on every system, and has
/// one spelling. Data names its scripts with one, so a path that leaves the package, or names a
/// file two ways, is refused where the data is read.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackagePath(String);

impl PackagePath {
    /// `text` as a path in a package; `None` when it does not hold to the grammar.
    pub fn parse(text: &str) -> Option<PackagePath> {
        let named =
            |name: &str| !name.is_empty() && name != "." && name != ".." && !name.contains('\\');
        text.split('/')
            .all(named)
            .then(|| PackagePath(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether it is under the directory `dir`, itself a path in the package.
    pub fn is_under(&self, dir: &str) -> bool {
        self.0
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
    }
}

impl<'de> Deserialize<'de> for PackagePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<PackagePath, D::Error> {
        let text = String::deserialize(deserializer)?;
        PackagePath::parse(&text)
            .ok_or_else(|| D::Error::custom(format!("{text:?}: no path in the package")))
    }
}

#[cfg(test)]
mod tests;
