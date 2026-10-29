use campfire_capabilities::Release;
use campfire_common::MapName;
use campfire_mode::MapData;
use campfire_values::PackagePath;
use serde::de::DeserializeSeed;
use serde::{Deserialize, Deserializer};

/// A map's `map/<name>/map.toml`, its sections read as the release's modules register them.
#[derive(Debug)]
pub struct MapFile(pub(crate) MapData);

impl MapFile {
    /// Where a mode's package holds the file of the map `map`.
    pub fn path(map: &MapName) -> PackagePath {
        PackagePath::parse(&format!("map/{map}/map.toml"))
            .expect("a map's name is a package path's name")
    }

    /// The name of the map whose file `path` is, as `path` spells it; none for any other path.
    pub(crate) fn map_of(path: &PackagePath) -> Option<&str> {
        let name = path
            .as_str()
            .strip_prefix("map/")?
            .strip_suffix("/map.toml")?;
        (!name.contains('/')).then_some(name)
    }
}

impl<'de> Deserialize<'de> for MapFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let readers = Release::sections();
        Ok(Self(MapData::seed(readers).deserialize(deserializer)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_s_file_is_under_its_name_and_only_such_a_path_names_a_map() {
        let alpine = MapName::parse("alpine").unwrap();
        let path = MapFile::path(&alpine);
        assert_eq!(path.as_str(), "map/alpine/map.toml");
        assert_eq!(MapFile::map_of(&path), Some("alpine"));
        for other in [
            "map/a/b/map.toml",
            "map/alpine/heights.bin",
            "maps/alpine/map.toml",
        ] {
            let path = PackagePath::parse(other).unwrap();
            assert_eq!(MapFile::map_of(&path), None, "{other}");
        }
    }
}
