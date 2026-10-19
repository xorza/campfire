use campfire_capabilities::Release;
use campfire_mode::MapData;
use serde::de::DeserializeSeed;
use serde::{Deserialize, Deserializer};

/// A map's `map/<name>/map.toml`, its sections read as the release's modules register them.
#[derive(Debug)]
pub(crate) struct MapFile(pub(crate) MapData);

impl<'de> Deserialize<'de> for MapFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let readers = Release::sections();
        Ok(Self(MapData::seed(&readers).deserialize(deserializer)?))
    }
}
