use serde::Deserialize;

use crate::files::mode_manifest::ModeManifest;
use crate::files::package_header::LocaleManifest;
use crate::files::package_header::PackageHeader;

/// A package's `manifest.toml`: what it is, which package API version it targets, and, for a mode,
/// the rules its matches run by.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Manifest {
    Mode(ModeManifest),
    Avatar(PackageHeader),
    Loadout(PackageHeader),
    Locale(LocaleManifest),
}

impl Manifest {
    pub(crate) const fn header(&self) -> &PackageHeader {
        match self {
            Manifest::Mode(mode) => &mode.header,
            Manifest::Avatar(header) | Manifest::Loadout(header) => header,
            Manifest::Locale(locale) => &locale.header,
        }
    }
}
