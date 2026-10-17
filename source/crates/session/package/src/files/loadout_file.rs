use campfire_capabilities::{PackageContent, Release};
use serde::de::DeserializeSeed;
use serde::{Deserialize, Deserializer};

/// A loadout package's `data/loadout.toml`: its content, its unit types' sections read as the
/// release's modules register them.
#[derive(Debug)]
pub(crate) struct LoadoutFile(pub(crate) PackageContent);

impl<'de> Deserialize<'de> for LoadoutFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let readers = Release::sections();
        Ok(Self(
            PackageContent::seed(&readers).deserialize(deserializer)?,
        ))
    }
}
