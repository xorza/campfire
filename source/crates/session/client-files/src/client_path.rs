//! The package paths of the client's files.

use campfire_common::MapName;
use campfire_values::PackagePath;

/// The client file `file` of the map `map`, `client/maps/<map>/<file>`.
pub(crate) fn of_map(map: &MapName, file: &str) -> PackagePath {
    parse(&format!("client/maps/{map}/{file}"))
}

/// The path `text` spells, one of the client's files, of names a map's name is one of.
pub(crate) fn parse(text: &str) -> PackagePath {
    PackagePath::parse(text).expect("a client file's path is a package path")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera_file::CameraFile;
    use crate::client_objects::ClientObjects;
    use crate::client_units::ClientUnits;
    use crate::height_grid::HeightGrid;
    use crate::map_conditions::MapConditions;
    use crate::map_lights::MapLights;
    use crate::material_file::MaterialFile;
    use crate::terrain_file::TerrainFile;

    #[test]
    fn each_client_file_is_at_its_path() {
        // Each file of a map at its path, and the package's own client files at theirs.
        let map = MapName::parse("tournament_desert").unwrap();
        let paths = [
            ClientObjects::path(&map),
            MapLights::path(&map),
            MapConditions::path(&map),
            TerrainFile::path(&map),
            HeightGrid::path(&map),
            ClientUnits::path(),
            CameraFile::path(),
        ];
        assert_eq!(
            paths.each_ref().map(PackagePath::as_str),
            [
                "client/maps/tournament_desert/objects.toml",
                "client/maps/tournament_desert/lights.toml",
                "client/maps/tournament_desert/conditions.toml",
                "client/maps/tournament_desert/terrain.bin",
                "map/tournament_desert/heights.bin",
                "client/units.toml",
                "client/camera.toml",
            ]
        );
        // A material's file by its name, and its name by its file; a name no path holds, and a
        // file elsewhere or of another kind, are none.
        let flash = MaterialFile::path("tank_3").unwrap();
        assert_eq!(flash.as_str(), "client/materials/tank_3.toml");
        assert_eq!(MaterialFile::name_of(&flash), Some("tank_3"));
        assert_eq!(MaterialFile::path("a:b"), None);
        for other in [
            "client/materials/notes.txt",
            "client/units.toml",
            "client/materialsx/a.toml",
        ] {
            assert_eq!(
                MaterialFile::name_of(&PackagePath::parse(other).unwrap()),
                None,
                "{other}"
            );
        }
    }
}
