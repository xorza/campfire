use campfire_capabilities::{PackageContent, Release};
use campfire_declarations::UnitDefaults;
use serde::Deserializer;
use serde::de::DeserializeSeed;

/// A loadout package's `data/loadout.toml`: its content, its unit types' sections read as the
/// release's modules register them, the mode's defaults under them.
#[derive(Debug)]
pub(crate) struct LoadoutFile(pub(crate) PackageContent);

/// What reads a loadout's file, as serde's seed, of the mode's `defaults`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoadoutSeed<'d>(pub(crate) &'d UnitDefaults);

impl<'de> DeserializeSeed<'de> for LoadoutSeed<'_> {
    type Value = LoadoutFile;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<LoadoutFile, D::Error> {
        let readers = Release::sections();
        let content = PackageContent::seed(readers, self.0).deserialize(deserializer)?;
        Ok(LoadoutFile(content))
    }
}
