use std::collections::BTreeMap;

use campfire_capabilities::ApiVersion;
use serde::{Deserialize, Deserializer};

use crate::files::dependency::Dependency;
use crate::files::version::Version;
use crate::language::Language;

/// What every manifest starts with: the package's name and version, the package API version it
/// targets, and the language of its own text.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageHeader {
    pub name: String,
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
    pub dependencies: BTreeMap<String, Dependency>,
}

/// The flat table of a locale package's manifest, its header's fields beside its dependencies.
impl<'de> Deserialize<'de> for LocaleManifest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<LocaleManifest, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            name: String,
            version: Version,
            api: ApiVersion,
            language: Language,
            dependencies: BTreeMap<String, Dependency>,
        }
        let fields = Fields::deserialize(deserializer)?;
        Ok(LocaleManifest {
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
