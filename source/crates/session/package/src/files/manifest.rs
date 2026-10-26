use campfire_package_format::PackageFiles;
use campfire_script::ApiVersion;
use campfire_sim::Capability;
use serde::Deserialize;

use crate::error::load_problem::LoadProblem;
use crate::error::{LoadError, PackageRef};
use campfire_values::PackageName;

use crate::files::mode_manifest::ModeManifest;
use crate::files::package_header::LocaleManifest;
use crate::files::package_header::PackageHeader;
use crate::files::rules_manifest::RulesManifest;

/// A package's `manifest.toml`: what it is, which package API version it targets, and, for a mode,
/// the rules its matches run by.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Manifest {
    Mode(ModeManifest<Vec<Capability>>),
    Avatar(PackageHeader),
    Loadout(PackageHeader),
    Locale(LocaleManifest),
    /// Scripts a mode that depends on it names as its own, and data its own lies over.
    Rules(RulesManifest),
}

impl Manifest {
    /// The manifest of `files`, the package `package`, once its API version is one the release
    /// loads: the version and the name are read first, as every version of the format names both,
    /// so a package of another version fails as `OtherApi` before any other field or file of it
    /// is read (03-game-scripting.md, A versioned package API).
    pub(crate) fn read(files: &PackageFiles, package: &PackageRef) -> Result<Self, LoadError> {
        #[derive(Debug, Deserialize)]
        struct Targets {
            name: PackageName,
            api: ApiVersion,
        }
        let path = PackageFiles::manifest_path();
        if let Ok(Targets { name, api }) = files.read_data(&path)
            && !ApiVersion::RELEASE.loads(api)
        {
            return Err(LoadError::of(&name, LoadProblem::OtherApi(api)));
        }
        files
            .read_data(&path)
            .map_err(|error| LoadError::new(package.clone(), LoadProblem::Content(error)))
    }

    /// The manifest of `files`, a mode's, as the package `package`; an error for a manifest that
    /// does not read, or of another kind of package.
    pub(crate) fn read_mode(
        files: &PackageFiles,
        package: &PackageRef,
    ) -> Result<ModeManifest, LoadError> {
        let fail = |problem| LoadError::new(package.clone(), problem);
        match Self::read(files, package)? {
            Self::Mode(manifest) => manifest
                .checked()
                .map_err(|error| fail(LoadProblem::Capabilities(error))),
            _ => Err(fail(LoadProblem::WrongKind)),
        }
    }

    pub(crate) const fn header(&self) -> &PackageHeader {
        match self {
            Self::Mode(mode) => &mode.header,
            Self::Avatar(header) | Self::Loadout(header) => header,
            Self::Rules(rules) => &rules.header,
            Self::Locale(locale) => &locale.header,
        }
    }
}
