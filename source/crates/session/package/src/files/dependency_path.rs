use campfire_values::PackagePath;
use derive_more::Display;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A dependency's path in the workspace, relative to the mode's manifest: names joined by `/`,
/// each `..` or a name a package path takes, so it names no root, no drive and no device, and
/// reads as the same directory on every OS.
#[derive(Debug, Display, Clone, PartialEq, Eq)]
pub struct DependencyPath(String);

impl DependencyPath {
    /// `text` as a dependency's path; none when a name of it is neither `..` nor a package
    /// path's name.
    pub fn parse(text: &str) -> Option<Self> {
        text.split('/')
            .all(|name| name == ".." || PackagePath::parse(name).is_some())
            .then(|| Self(text.to_owned()))
    }

    /// Its names, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
}

/// As its text, which `Deserialize` reads.
impl Serialize for DependencyPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DependencyPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| {
            D::Error::custom(format!("{text:?}: no path relative to the mode's manifest"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dependency_path_names_only_directories_and_their_parents() {
        let path = DependencyPath::parse("../../heroes/lancer").unwrap();
        assert_eq!(
            path.names().collect::<Vec<_>>(),
            ["..", "..", "heroes", "lancer"]
        );
        assert!(DependencyPath::parse("rules").is_some());
        // A root, a drive, a share, a backslash, an empty name, `.`, a device and a name Windows
        // drops the end of: each a way out of the workspace or another directory on another OS.
        for refused in [
            "/rules",
            "C:/rules",
            "//host/share",
            "..\\rules",
            "a//b",
            "",
            "./rules",
            "../aux",
            "rules.",
        ] {
            assert_eq!(DependencyPath::parse(refused), None, "{refused:?}");
        }
    }
}
