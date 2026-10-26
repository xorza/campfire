use campfire_common::Version;
use campfire_script::ApiVersion;
use campfire_sim::Capability;
use serde::{Deserialize, Deserializer};

use crate::files::package_header::PackageHeader;
use crate::files::rate_bounds::RateBounds;
use crate::language::Language;
use campfire_values::PackageName;

/// A rules package's manifest: its header's fields, and what it needs of a mode that depends on
/// it, in one table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RulesManifest {
    pub header: PackageHeader,
    pub needs: RulesNeeds,
}

/// What a rules package needs of a mode that depends on it, which the load refuses a mode that
/// lacks (03-game-scripting.md, A rules package states what it needs): the capabilities its
/// scripts and data use, and the tick rates they run at, any with none named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesNeeds {
    pub capabilities: Vec<Capability>,
    pub tick_hz: Option<RateBounds>,
}

/// The flat table of a rules package's manifest, its header's fields beside its needs.
impl<'de> Deserialize<'de> for RulesManifest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            name: PackageName,
            version: Version,
            api: ApiVersion,
            language: Language,
            #[serde(default)]
            capabilities: Vec<Capability>,
            tick_hz: Option<RateBounds>,
        }
        let fields = Fields::deserialize(deserializer)?;
        Ok(Self {
            header: PackageHeader {
                name: fields.name,
                version: fields.version,
                api: fields.api,
                language: fields.language,
            },
            needs: RulesNeeds {
                capabilities: fields.capabilities,
                tick_hz: fields.tick_hz,
            },
        })
    }
}
