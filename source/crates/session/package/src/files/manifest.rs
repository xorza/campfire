use campfire_sim::Capability;
use serde::Deserialize;

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
    pub(crate) const fn header(&self) -> &PackageHeader {
        match self {
            Self::Mode(mode) => &mode.header,
            Self::Avatar(header) | Self::Loadout(header) => header,
            Self::Rules(rules) => &rules.header,
            Self::Locale(locale) => &locale.header,
        }
    }
}
