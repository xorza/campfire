use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use bevy::app::{App, Plugin};
use bevy::ecs::resource::Resource;
use campfire_client_files::{
    CameraFile, ClientObjects, ClientUnits, MapConditions, MapLights, MaterialFile, TerrainFile,
};
use campfire_codec::{Binary, Toml, TomlError};
use campfire_common::MapName;
use campfire_log::ErrorReport;
use campfire_package::{ContentError, ModePackages, PackageReader};
use campfire_units::HeightGrid;
use campfire_values::{PackageIndex, PackagePath};
use tracing::error;

use crate::view::ground_heights::GroundHeights;

/// The client data of the mode's packages, which the view draws units by: each package's unit
/// looks, `client/units.toml`, and its material files, `client/materials/<name>.toml`, by their
/// names, each by the package's place; the map's heightmap and the ground's heights from it; and
/// the mode's own package's camera, `client/camera.toml`, the map's lights,
/// `client/maps/<map>/lights.toml`, its client objects, `client/maps/<map>/objects.toml`,
/// whose looks are its package's, and its terrain, `client/maps/<map>/terrain.bin`, kept when its
/// quads are the heights' cells. Each file is read once, as the view
/// starts, and checked against its package's index; one that does not read is logged, and drawn
/// as if it were not there.
#[derive(Resource, Debug, Default)]
pub(crate) struct ClientData {
    pub(crate) units: Vec<ClientUnits>,
    pub(crate) materials: Vec<BTreeMap<String, MaterialFile>>,
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

impl ClientData {
    pub(crate) fn read(packages: &ModePackages) -> Self {
        let mut data = Self::default();
        for view in packages.packages() {
            data.read_package(view.index, &view.package.files);
        }
        data.read_map(&packages.mode().package.files, packages.map_name());
        data
    }

    /// Reads the mode's own package's files of `map`: its heights and its terrain over them, the
    /// camera, the map's lights and its client objects; and its conditions, by which each look
    /// takes its variant.
    fn read_map(&mut self, mode: &PackageReader, map: &MapName) {
        let heights = HeightGrid::path(map);
        let grid = Self::read_file(mode, &heights)
            .and_then(|bytes| Self::logged(&heights, Binary::decode::<HeightGrid>(&bytes)));
        let terrain = TerrainFile::path(map);
        self.terrain = grid.as_ref().and_then(|grid| {
            let bytes = Self::read_file(mode, &terrain)?;
            let file = Self::logged(&terrain, Binary::decode::<TerrainFile>(&bytes))?;
            let fits = file.quads().fits(grid.columns(), grid.rows());
            if !fits {
                error!(path = %terrain, "a map's terrain is not the shape of its heights");
            }
            fits.then_some(file)
        });
        let quads = self.terrain.as_ref().map(TerrainFile::quads);
        self.heights = grid.map(|grid| GroundHeights::of(grid, quads));
        self.camera = Self::toml(mode, &CameraFile::path(), Toml::parse);
        self.lights = Self::toml(mode, &MapLights::path(map), Toml::parse);
        let objects = ClientObjects::path(map);
        self.objects = Self::toml(mode, &objects, Toml::parse).unwrap_or_default();
        // The map's conditions are fixed for the match, so each look takes its variant once.
        let conditions = MapConditions::path(map);
        let conditions = Self::toml::<MapConditions>(mode, &conditions, Toml::parse)
            .unwrap_or_default()
            .conditions;
        for units in &mut self.units {
            for look in units.units.values_mut().chain(units.objects.values_mut()) {
                look.settle(&conditions);
            }
        }
    }

    fn read_package(&mut self, place: PackageIndex, files: &PackageReader) {
        debug_assert_eq!(
            usize::from(place.get()),
            self.units.len(),
            "packages read in their places' order"
        );
        let units = Self::toml::<ClientUnits>(files, &ClientUnits::path(), Toml::parse);
        self.units.push(units.unwrap_or_default());
        let mut materials = BTreeMap::new();
        for path in files.files_under(MaterialFile::DIR) {
            let Some(name) = MaterialFile::name_of(path) else {
                continue;
            };
            let file = Self::toml::<MaterialFile>(files, path, Toml::parse);
            if let Some(file) = file {
                materials.insert(name.to_owned(), file);
            }
        }
        self.materials.push(materials);
    }

    /// The unit looks of the package at `place`.
    pub(crate) fn units_of(&self, place: PackageIndex) -> &ClientUnits {
        &self.units[usize::from(place.get())]
    }

    /// The material files of the package at `place`, by their names.
    pub(crate) fn materials_of(&self, place: PackageIndex) -> &BTreeMap<String, MaterialFile> {
        &self.materials[usize::from(place.get())]
    }

    /// The bytes of the file at `path`; none for a file the package lacks, and none, logged, for
    /// one that does not read.
    pub(super) fn read_file(files: &PackageReader, path: &PackagePath) -> Option<Vec<u8>> {
        match files.read_file(path) {
            Ok(bytes) => Some(bytes),
            Err(ContentError::Missing { .. }) => None,
            Err(problem) => Self::logged(path, Err::<Vec<u8>, _>(problem)),
        }
    }

    /// The TOML file at `path` as `parse` reads it; none for a file the package lacks, and none,
    /// logged, for one that does not read.
    fn toml<T>(
        files: &PackageReader,
        path: &PackagePath,
        parse: fn(&str) -> Result<T, TomlError>,
    ) -> Option<T> {
        let text = Self::text(path, Self::read_file(files, path)?)?;
        Self::logged(path, parse(&text))
    }

    fn text(path: &PackagePath, bytes: Vec<u8>) -> Option<String> {
        Self::logged(path, String::from_utf8(bytes))
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
