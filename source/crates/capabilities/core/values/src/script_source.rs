use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::package_name::PackageName;
use crate::package_path::PackagePath;

/// A script that data names: one of the naming package's own, by its path, or one of a rules
/// package its mode depends on, `{ package, path }`, as a mode's list and a unit type's AI name
/// theirs (03-game-scripting.md, Rules scripts in order; 03-game-scripting.md, A unit type's AI
/// may be a rules package's script).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptSource {
    Own(PackagePath),
    Rules {
        package: PackageName,
        path: PackagePath,
    },
}

impl ScriptSource {
    /// The script at `path` of `package`, the naming package's own with none.
    pub fn of(package: Option<PackageName>, path: PackagePath) -> Self {
        match package {
            Some(package) => Self::Rules { package, path },
            None => Self::Own(path),
        }
    }

    /// The script's path in its package.
    pub const fn path(&self) -> &PackagePath {
        match self {
            Self::Own(path) | Self::Rules { path, .. } => path,
        }
    }

    /// The rules package that holds it; none for the naming package's own.
    pub const fn package(&self) -> Option<&PackageName> {
        match self {
            Self::Own(_) => None,
            Self::Rules { package, .. } => Some(package),
        }
    }
}

/// A path alone, or `{ package, path }`, which `Deserialize` reads back to it.
impl Serialize for ScriptSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Own(path) => path.serialize(serializer),
            Self::Rules { package, path } => Source {
                package: Some(package.clone()),
                path: path.clone(),
            }
            .serialize(serializer),
        }
    }
}

/// A path alone, or a table of an optional package and a path.
impl<'de> Deserialize<'de> for ScriptSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match WrittenSource::deserialize(deserializer)? {
            WrittenSource::Own(path) => Self::Own(path),
            WrittenSource::Table(Source { package, path }) => Self::of(package, path),
        })
    }
}

/// A script as a file writes it: a path alone, or a table.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WrittenSource {
    Own(PackagePath),
    Table(Source),
}

/// A script in its table's form, which reads and writes the same keys.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<PackageName>,
    path: PackagePath,
}
