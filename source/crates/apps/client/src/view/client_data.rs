use std::collections::BTreeMap;
use std::error::Error;

use bevy::ecs::resource::Resource;
use campfire_codec::{Binary, Toml, TomlError};
use campfire_common::MapName;
use campfire_log::ErrorReport;
use campfire_mode::HeightGrid;
use campfire_package::{
    CameraFile, ClientObjects, ClientUnits, ContentError, MapLights, MaterialFile, ModePackages,
    PackageReader,
};
use campfire_values::PackagePath;
use tracing::error;

use crate::view::ground_heights::GroundHeights;

/// The client data of the mode's packages, which the view draws units by: each package's unit
/// looks, `client/units.toml`, by its place, and every material file, `client/materials/<name>.toml`,
/// by its package's place and its name; the map's heightmap and the ground's heights from it; and
/// the mode's own package's camera, `client/camera.toml`, the map's lights,
/// `client/maps/<map>/lights.toml`, and its client objects, `client/maps/<map>/objects.toml`,
/// whose looks are its package's. Each file is read once, as the view
/// starts, and checked against its package's index; one that does not read is logged, and drawn
/// as if it were not there.
#[derive(Resource, Debug, Default)]
pub(crate) struct ClientData {
    pub(crate) units: Vec<ClientUnits>,
    pub(crate) materials: BTreeMap<(u16, String), MaterialFile>,
    pub(crate) grid: Option<HeightGrid>,
    pub(crate) heights: Option<GroundHeights>,
    /// The mode's own package's camera file.
    pub(crate) camera: Option<CameraFile>,
    /// The map's lights, which its units are drawn in.
    pub(crate) lights: Option<MapLights>,
    /// The map's client objects, none when it has no file of them.
    pub(crate) objects: ClientObjects,
}

const UNITS: &str = "client/units.toml";
const MATERIALS: &str = "client/materials";

impl ClientData {
    pub(crate) fn read(packages: &ModePackages) -> ClientData {
        let mut data = ClientData::default();
        for (place, view) in packages.packages().enumerate() {
            let place = u16::try_from(place).expect("a mode's packages fit u16");
            data.read_package(place, &view.package.files);
        }
        let mode = &packages
            .packages()
            .next()
            .expect("a mode has its own package")
            .package
            .files;
        data.read_map(mode, packages.map_name());
        data
    }

    /// Reads the mode's own package's files of `map`: its heights, the camera, the map's lights
    /// and its client objects.
    fn read_map(&mut self, mode: &PackageReader, map: &MapName) {
        let heights = format!("map/{map}/heights.bin");
        let heights = PackagePath::parse(&heights).expect("a map's name is a package path's");
        self.grid = ClientData::read_file(mode, &heights)
            .and_then(|bytes| ClientData::logged(&heights, Binary::decode::<HeightGrid>(&bytes)));
        self.heights = self.grid.as_ref().and_then(GroundHeights::of);
        self.camera = ClientData::toml(mode, CameraFile::PATH, Toml::parse);
        self.lights = ClientData::toml(mode, &MapLights::path(map.as_str()), Toml::parse);
        let objects = ClientObjects::path(map.as_str());
        self.objects = ClientData::toml(mode, &objects, Toml::parse).unwrap_or_default();
    }

    fn read_package(&mut self, place: u16, files: &PackageReader) {
        let units = ClientData::toml::<ClientUnits>(files, UNITS, Toml::parse);
        self.units.push(units.unwrap_or_default());
        for path in files.files_under(MATERIALS) {
            let Some(name) = path
                .as_str()
                .strip_prefix(MATERIALS)
                .and_then(|rest| rest.strip_prefix('/'))
                .and_then(|file| file.strip_suffix(".toml"))
            else {
                continue;
            };
            let file = ClientData::toml::<MaterialFile>(files, path.as_str(), Toml::parse);
            if let Some(file) = file {
                self.materials.insert((place, name.to_owned()), file);
            }
        }
    }

    /// The bytes of the file at `path`; none for a file the package lacks, and none, logged, for
    /// one that does not read.
    fn read_file(files: &PackageReader, path: &PackagePath) -> Option<Vec<u8>> {
        match files.read_file(path) {
            Ok(bytes) => Some(bytes),
            Err(ContentError::Missing { .. }) => None,
            Err(problem) => ClientData::logged(path, Err::<Vec<u8>, _>(problem)),
        }
    }

    /// The TOML file at `path` as `parse` reads it; none for a file the package lacks, and none,
    /// logged, for one that does not read.
    fn toml<T>(
        files: &PackageReader,
        path: &str,
        parse: fn(&str) -> Result<T, TomlError>,
    ) -> Option<T> {
        let path = PackagePath::parse(path).expect("a client data file's path is a package path");
        let text = ClientData::text(&path, ClientData::read_file(files, &path)?)?;
        ClientData::logged(&path, parse(&text))
    }

    fn text(path: &PackagePath, bytes: Vec<u8>) -> Option<String> {
        ClientData::logged(path, String::from_utf8(bytes))
    }

    /// `result`'s value; none, logged with `path`, for its error.
    fn logged<T, E: Error + 'static>(path: &PackagePath, result: Result<T, E>) -> Option<T> {
        result
            .map_err(|problem| error!(path = %path, error = %ErrorReport::of(&problem), "a client data file does not read"))
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Num;
    use campfire_package::Blend;
    use campfire_store::internals::Scratch;

    use super::*;
    use crate::view::package_source::internals::package;

    #[test]
    fn a_package_gives_its_units_and_its_material_files_and_skips_what_does_not_read() {
        let scratch = Scratch::new();
        let units = br#"[units.tank]
models = [{ model = "client/models/tank.glb", hide = ["tank.flash"] }]
"#;
        let flash = br#"base_color = [1.0, 1.0, 1.0, 1.0]
emissive = [1.0, 0.0, 0.0]
metallic = 0.0
roughness = 1.0
specular = 0.0
unlit = false
blend = { mode = "add" }
depth_write = false
depth_bias = 0
double_sided = true
"#;
        let files = package(
            &scratch,
            "mode",
            &[
                ("client/units.toml", units),
                ("client/materials/tank_3.toml", flash),
                ("client/materials/broken.toml", b"blend = 1"),
                ("client/materials/notes.txt", b"no material"),
            ],
        );
        let none = package(&scratch, "bare", &[("data/units.toml", b"")]);
        let mut data = ClientData::default();
        data.read_package(0, &files);
        data.read_package(1, &none);
        // The tank's model, with the node it hides; a package with no units has none.
        let tank = &data.units[0].units["tank"].models[0];
        assert_eq!(
            (tank.model.as_str(), tank.hide.as_slice()),
            ("client/models/tank.glb", &["tank.flash".to_owned()][..])
        );
        assert!(data.units[1].units.is_empty());
        // The material file by its package and name; the one that does not read, and the file
        // that is no TOML, are not there.
        let keys: Vec<&(u16, String)> = data.materials.keys().collect();
        assert_eq!(keys, [&(0, "tank_3".to_owned())]);
        assert_eq!(data.materials[&(0, "tank_3".to_owned())].blend, Blend::Add);
    }

    #[test]
    fn a_map_gives_its_lights_and_its_client_objects_and_skips_what_does_not_read() {
        let scratch = Scratch::new();
        let lights = b"ambient = [0.25, 0.25, 0.25]
lights = [{ color = [1.0, 1.0, 1.0], direction = [0.0, -1.0, 0.0] }]
";
        let objects = br#"[[objects]]
look = "oak"
pos = [3, "-2.5"]
height = 1
angle = 90
"#;
        let map = MapName::new("desert").unwrap();
        let mode = package(
            &scratch,
            "mode",
            &[
                ("client/maps/desert/lights.toml", lights),
                ("client/maps/desert/objects.toml", objects),
            ],
        );
        let mut data = ClientData::default();
        data.read_map(&mode, &map);
        // The map's lights, and its one oak at (3, −2.5), 1 m up, turned 90°.
        assert_eq!(data.lights.unwrap().lights.len(), 1);
        let oak = &data.objects.objects[0];
        assert_eq!(data.objects.objects.len(), 1);
        assert_eq!(oak.look.as_str(), "oak");
        assert_eq!(oak.pos, [Num::int(3), Num::int(-5) / Num::int(2)]);
        assert_eq!((oak.height, oak.angle), (Num::ONE, Num::int(90)));
        // A map of no objects file has none, as one whose file does not read; no heights or
        // camera file gives none either.
        let broken = package(
            &scratch,
            "broken",
            &[(
                "client/maps/desert/objects.toml",
                b"[[objects]]\nlook = 1\n",
            )],
        );
        for files in [package(&scratch, "bare", &[]), broken] {
            let mut data = ClientData::default();
            data.read_map(&files, &map);
            assert!(data.objects.objects.is_empty());
            assert!(data.lights.is_none() && data.camera.is_none() && data.heights.is_none());
        }
    }
}
