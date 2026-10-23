use std::collections::BTreeMap;

use campfire_common::Version;
use campfire_script::ApiVersion;
use serde::{Deserialize, Deserializer, Serialize};

use crate::files::dependency::Dependency;
use crate::language::Language;
use crate::package_name::PackageName;

/// What every manifest starts with: the package's name and version, the package API version it
/// targets, and the language of its own text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageHeader {
    pub name: PackageName,
    pub version: Version,
    pub api: ApiVersion,
    pub language: Language,
}

/// A locale package's manifest: its header's fields, and the packages it translates, in one
/// table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocaleManifest {
    pub header: PackageHeader,
    /// By name; in the workspace each is a path, relative to the manifest.
    pub dependencies: BTreeMap<PackageName, Dependency>,
}

/// The flat table of a locale package's manifest, its header's fields beside its dependencies.
impl<'de> Deserialize<'de> for LocaleManifest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            name: PackageName,
            version: Version,
            api: ApiVersion,
            language: Language,
            dependencies: BTreeMap<PackageName, Dependency>,
        }
        let fields = Fields::deserialize(deserializer)?;
        Ok(Self {
            header: PackageHeader {
                name: fields.name,
                version: fields.version,
                api: fields.api,
                language: fields.language,
            },
            dependencies: fields.dependencies,
        })
    }
}
