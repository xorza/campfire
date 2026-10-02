use std::fmt;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A path to a file inside a package: names joined by `/`, each one not empty, not `.` or `..`,
/// and holding no `\`, so it cannot leave the package, reads the same on every system, and has
/// one spelling. Data names its scripts with one, so a path that leaves the package, or names a
/// file two ways, is refused where the data is read.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

impl fmt::Display for PackagePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
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
mod tests {
    use super::*;

    #[test]
    fn a_path_is_plain_names_joined_by_slashes_with_one_spelling() {
        for text in [
            "manifest.toml",
            "scripts/a.rhai",
            "data/units.toml",
            "a b/c-d.rhai",
        ] {
            assert_eq!(
                PackagePath::parse(text).map(|path| path.0),
                Some(text.to_owned())
            );
        }
        for text in [
            "",
            "/scripts/a.rhai",
            "scripts/",
            "scripts//a.rhai",
            "scripts/./a.rhai",
            "./scripts/a.rhai",
            "scripts/../a.rhai",
            "..",
            "scripts\\a.rhai",
            "C:\\scripts\\a.rhai",
        ] {
            assert_eq!(PackagePath::parse(text), None, "{text:?}");
        }
        let script = PackagePath::parse("scripts/ai/think.rhai").unwrap();
        assert!(script.is_under("scripts") && script.is_under("scripts/ai"));
        assert!(!script.is_under("script") && !script.is_under("scripts/ai/think.rhai"));
    }
}
