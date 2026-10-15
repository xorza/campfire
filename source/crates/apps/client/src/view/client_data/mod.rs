use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use bevy::app::{App, Plugin};
use bevy::ecs::resource::Resource;
use campfire_client_files::HeightGrid;
use campfire_client_files::{
    CameraFile, ClientObjects, ClientUnits, MapLights, MaterialFile, TerrainFile,
};
use campfire_codec::{Binary, Toml, TomlError};
use campfire_common::MapName;
use campfire_log::ErrorReport;
use campfire_package::{ContentError, ModePackages, PackageIndex, PackageReader};
use campfire_values::PackagePath;
use tracing::error;

use crate::view::ground_heights::GroundHeights;

/// The client data of the mode's packages, which the view draws units by: each package's unit
/// looks, `client/units.toml`, by its place, and every material file, `client/materials/<name>.toml`,
/// by its package's place and its name; the map's heightmap and the ground's heights from it; and
/// the mode's own package's camera, `client/camera.toml`, the map's lights,
/// `client/maps/<map>/lights.toml`, its client objects, `client/maps/<map>/objects.toml`,
/// whose looks are its package's, and its terrain, `client/maps/<map>/terrain.bin`, kept when its
/// quads are the heights' cells. Each file is read once, as the view
/// starts, and checked against its package's index; one that does not read is logged, and drawn
/// as if it were not there.
#[derive(Resource, Debug, Default)]
pub(crate) struct ClientData {
    pub(crate) units: Vec<ClientUnits>,
    pub(crate) materials: BTreeMap<(PackageIndex, String), MaterialFile>,
    /// The ground's heights from the map's heightmap; none for a map of no heightmap.
    pub(crate) heights: Option<GroundHeights>,
    /// The map's terrain over its heights; none for a map of no heights, of no terrain file, or
    /// of one that does not read or is not the heights' shape.
    pub(crate) terrain: Option<TerrainFile>,
    /// The mode's own package's camera file.
    pub(crate) camera: Option<CameraFile>,
    /// The map's lights, which its units are drawn in.
    pub(crate) lights: Option<MapLights>,
    /// The map's client objects, none when it has no file of them.
    pub(crate) objects: ClientObjects,
}

/// Reads the mode's client data as the client starts: the one source of `ClientData`, which the
/// view, the camera, the models, the terrain and the pointer read.
#[derive(Debug)]
pub(crate) struct ClientFiles {
    pub(crate) packages: Arc<ModePackages>,
}

impl Plugin for ClientFiles {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClientData::read(&self.packages));
    }
}

const UNITS: &str = "client/units.toml";
const MATERIALS: &str = "client/materials";

impl ClientData {
    pub(crate) fn read(packages: &ModePackages) -> ClientData {
        let mut data = ClientData::default();
        for view in packages.packages() {
            data.read_package(view.index, &view.package.files);
        }
        data.read_map(&packages.mode().package.files, packages.map_name());
        data
    }

    /// Reads the mode's own package's files of `map`: its heights and its terrain over them, the
    /// camera, the map's lights and its client objects.
    fn read_map(&mut self, mode: &PackageReader, map: &MapName) {
        let heights = format!("map/{map}/heights.bin");
        let heights = PackagePath::parse(&heights).expect("a map's name is a package path's");
        let grid = ClientData::read_file(mode, &heights)
            .and_then(|bytes| ClientData::logged(&heights, Binary::decode::<HeightGrid>(&bytes)));
        let terrain = PackagePath::parse(&TerrainFile::path(map.as_str()))
            .expect("a map's name is a package path's");
        self.terrain = grid.as_ref().and_then(|grid| {
            let bytes = ClientData::read_file(mode, &terrain)?;
            let file = ClientData::logged(&terrain, Binary::decode::<TerrainFile>(&bytes))?;
            let fits = file.quads().fits(grid);
            if !fits {
                error!(path = %terrain, "a map's terrain is not the shape of its heights");
            }
            fits.then_some(file)
        });
        let quads = self.terrain.as_ref().map(TerrainFile::quads);
        self.heights = grid.map(|grid| GroundHeights::of(grid, quads));
        self.camera = ClientData::toml(mode, CameraFile::PATH, Toml::parse);
        self.lights = ClientData::toml(mode, &MapLights::path(map.as_str()), Toml::parse);
        let objects = ClientObjects::path(map.as_str());
        self.objects = ClientData::toml(mode, &objects, Toml::parse).unwrap_or_default();
    }

    fn read_package(&mut self, place: PackageIndex, files: &PackageReader) {
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

    /// The unit looks of the package at `place`.
    pub(crate) fn units_of(&self, place: PackageIndex) -> &ClientUnits {
        &self.units[usize::from(place.get())]
    }

    /// The bytes of the file at `path`; none for a file the package lacks, and none, logged, for
    /// one that does not read.
    pub(super) fn read_file(files: &PackageReader, path: &PackagePath) -> Option<Vec<u8>> {
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
    pub(super) fn logged<T, E: Error + 'static>(
        path: &PackagePath,
        result: Result<T, E>,
    ) -> Option<T> {
        result
            .map_err(|problem| error!(path = %path, error = %ErrorReport::of(&problem), "a client data file does not read"))
            .ok()
    }
}

#[cfg(test)]
mod tests;
