use std::collections::BTreeMap;
use std::error::Error;

use bevy::ecs::resource::Resource;
use campfire_capabilities::{HeightGrid, PackagePath};
use campfire_common::{Binary, Toml};
use campfire_log::ErrorReport;
use campfire_package::{ClientUnits, ContentError, MaterialFile, ModePackages, PackageReader};
use tracing::error;

use crate::view::ground_heights::GroundHeights;

/// The client data of the mode's packages, which the view draws units by: each package's unit
/// looks, `client/units.toml`, by its place, and every material file, `client/materials/<name>.toml`,
/// by its package's place and its name; and the ground's heights, from the map's heightmap. Each
/// file is read once, as the view starts, and checked against its package's index; one that does
/// not read is logged, and drawn as if it were not there.
#[derive(Resource, Debug, Default)]
pub(crate) struct ClientData {
    pub(crate) units: Vec<ClientUnits>,
    pub(crate) materials: BTreeMap<(u16, String), MaterialFile>,
    pub(crate) heights: Option<GroundHeights>,
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
        let heights = format!("map/{}/heights.bin", packages.map_name());
        let heights = PackagePath::parse(&heights).expect("a map's name is a package path's");
        data.heights = ClientData::read_file(mode, &heights)
            .and_then(|bytes| ClientData::logged(&heights, Binary::decode::<HeightGrid>(&bytes)))
            .and_then(|grid| GroundHeights::of(&grid));
        data
    }

    fn read_package(&mut self, place: u16, files: &PackageReader) {
        let units = PackagePath::parse(UNITS).expect("a constant package path");
        let units = ClientData::read_file(files, &units)
            .and_then(|bytes| ClientData::text(&units, bytes))
            .and_then(|text| ClientData::logged(&units, Toml::parse::<ClientUnits>(&text)));
        self.units.push(units.unwrap_or_default());
        for path in files.files_under(MATERIALS) {
            let Some(name) = path
                .as_str()
                .strip_prefix("client/materials/")
                .and_then(|file| file.strip_suffix(".toml"))
            else {
                continue;
            };
            let file = ClientData::read_file(files, path)
                .and_then(|bytes| ClientData::text(path, bytes))
                .and_then(|text| ClientData::logged(path, Toml::parse::<MaterialFile>(&text)));
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
    use campfire_package::Blend;
    use campfire_store::Scratch;

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
blend = { mode = "add" }
depth_write = false
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
}
