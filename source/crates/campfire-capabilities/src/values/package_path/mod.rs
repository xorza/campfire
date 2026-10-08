use derive_more::Display;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A path to a file inside a package: names joined by `/`, each one not empty, not `.` or `..`,
/// so it cannot leave the package and has one spelling, and each one a file name every OS holds
/// as the same file's (design 03, Package paths), so a package reads the same on every OS. Data
/// names its scripts with one, so a path that leaves the package, or names a file two ways, is
/// refused where the data is read.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackagePath(String);

impl PackagePath {
    /// `text` as a path in a package; `None` when it does not hold to the grammar.
    pub fn parse(text: &str) -> Option<PackagePath> {
        text.split('/')
            .all(PackagePath::portable)
            .then(|| PackagePath(text.to_owned()))
    }

    /// Whether `name` is one name of a path, which every OS holds as the same file's: printable
    /// ASCII alone, so no OS folds its case or normalizes it otherwise; not empty, and not ending
    /// in a dot or a space, which Windows drops, so neither `.` nor `..`; holding no character
    /// Windows refuses, of `REFUSED`; and no device name Windows reserves, with any extension and
    /// in any case.
    fn portable(name: &str) -> bool {
        const REFUSED: [char; 8] = ['\\', ':', '*', '?', '"', '<', '>', '|'];
        const RESERVED: [&str; 24] = [
            "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
            "COM7", "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
            "LPT8", "LPT9",
        ];
        let stem = name
            .split_once('.')
            .map_or(name, |(stem, _)| stem)
            .trim_end();
        !name.is_empty()
            && !name.ends_with(['.', ' '])
            && name
                .chars()
                .all(|c| (c.is_ascii_graphic() || c == ' ') && !REFUSED.contains(&c))
            && !RESERVED
                .iter()
                .any(|device| stem.eq_ignore_ascii_case(device))
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
