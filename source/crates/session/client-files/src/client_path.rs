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
    use crate::hud_file::HudFile;
    use crate::keys_file::KeysFile;
    use crate::map_conditions::MapConditions;
    use crate::map_lights::MapLights;
    use crate::team_colors::TeamColors;
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
            ClientUnits::path(),
            CameraFile::path(),
            KeysFile::path(),
            HudFile::path(),
            TeamColors::path(),
        ];
        assert_eq!(
            paths.each_ref().map(PackagePath::as_str),
            [
                "client/maps/tournament_desert/objects.toml",
                "client/maps/tournament_desert/lights.toml",
                "client/maps/tournament_desert/conditions.toml",
                "client/maps/tournament_desert/terrain.bin",
                "client/units.toml",
                "client/camera.toml",
                "client/keys.toml",
                "client/hud.toml",
                "client/team_colors.toml",
            ]
        );
    }
}
