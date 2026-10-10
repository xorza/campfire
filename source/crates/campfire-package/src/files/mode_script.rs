use campfire_capabilities::PackagePath;
use serde::{Deserialize, Deserializer};

use crate::files::package_name::PackageName;

/// The script a mode runs as its own, as its `data/mode.toml` names it: one of its package's, by
/// its path, or one of a rules package it depends on, `{ package, path }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModeScript {
    Own(PackagePath),
    Rules {
        package: PackageName,
        path: PackagePath,
    },
}

impl ModeScript {
    /// The script's path in its package.
    pub(crate) const fn path(&self) -> &PackagePath {
        match self {
            ModeScript::Own(path) | ModeScript::Rules { path, .. } => path,
        }
    }

    /// The rules package that holds it; none for the mode's own.
    pub(crate) const fn package(&self) -> Option<&PackageName> {
        match self {
            ModeScript::Own(_) => None,
            ModeScript::Rules { package, .. } => Some(package),
        }
    }
}

/// A path alone, or a table of a package and a path.
impl<'de> Deserialize<'de> for ModeScript {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ModeScript, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Written {
            Own(PackagePath),
            Rules(Named),
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Named {
            package: PackageName,
            path: PackagePath,
        }
        Ok(match Written::deserialize(deserializer)? {
            Written::Own(path) => ModeScript::Own(path),
            Written::Rules(Named { package, path }) => ModeScript::Rules { package, path },
        })
    }
}
