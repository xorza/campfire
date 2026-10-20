use std::error::Error;
use std::sync::Arc;

use bevy::app::{App, Plugin};
use bevy::ecs::resource::Resource;
use campfire_client_files::{
    CameraFile, ClientObjects, ClientUnits, HudFile, KeysFile, MapConditions, MapLights,
    TeamColors, TerrainFile,
};
use campfire_codec::{Binary, Toml};
use campfire_common::ErrorReport;
use campfire_common::MapName;
use campfire_geometry::Metric;
use campfire_package::{ContentError, ModePackages, PackageReader};
use campfire_units::HeightGrid;
use campfire_values::{PackageIndex, PackagePath};
use serde::de::DeserializeOwned;
use tracing::error;

use crate::client_packages::ClientPackages;
use crate::input_map::InputMap;
use crate::view::ground_heights::GroundHeights;

/// The client data of the mode's packages, which the view draws units by: each package's own, by
/// the package's place; the ground's heights from the map's heightmap, which the packages hold; and
/// the mode's own package's camera, `client/camera.toml`, HUD, `client/hud.toml`, and team colors,
/// `client/team_colors.toml`, the map's lights, `client/maps/<map>/lights.toml`, its client
/// objects, `client/maps/<map>/objects.toml`, whose looks are its package's, and its terrain,
/// `client/maps/<map>/terrain.bin`, kept when its quads are the heights' cells. Each file is read
/// once, as the view starts, and checked against its package's index; one that does not read is
/// logged, and drawn as if it were not there.
#[derive(Resource, Debug, Default)]
pub(crate) struct ClientData {
    pub(crate) packages: Vec<PackageClient>,
    /// The ground's heights from the map's heightmap; none for a map of no heightmap.
    pub(crate) heights: Option<GroundHeights>,
    /// How the map reads a position: above its ground, or as a point in space.
    pub(crate) metric: Metric,
    /// The map's terrain over its heights; none for a map of no heights, of no terrain file, or
    /// of one that does not read or is not the heights' shape.
    pub(crate) terrain: Option<TerrainFile>,
    /// The mode's own package's camera file.
    pub(crate) camera: Option<CameraFile>,
    /// The mode's own package's HUD file, its default for a package of none.
    pub(crate) hud: HudFile,
    /// The mode's own package's team colors, the engine's for a package of none.
    pub(crate) team_colors: TeamColors,
    /// The map's lights, which its units are drawn in.
    pub(crate) lights: Option<MapLights>,
    /// The map's client objects, none when it has no file of them.
    pub(crate) objects: ClientObjects,
}

/// A package's client data: its unit looks, `client/units.toml`.
#[derive(Debug, Default)]
pub(crate) struct PackageClient {
    pub(crate) units: ClientUnits,
}

/// Reads the mode's client data as the client starts: the one source of `ClientData`, which the
/// view, the camera, the models, the terrain and the pointer read, of `ClientPackages`, and of
/// the `InputMap`, the mode's own package's `client/keys.toml`.
#[derive(Debug)]
pub(crate) struct ClientFilesPlugin {
    pub(crate) packages: Arc<ModePackages>,
}

impl Plugin for ClientFilesPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClientData::read(&self.packages));
        app.insert_resource(ClientPackages::of(&self.packages));
        let mode = &self.packages.mode().package.files;
        let input = ClientData::toml::<KeysFile>(mode, &KeysFile::path()).unwrap_or_default();
        app.insert_resource(InputMap::new(input));
    }
}

impl ClientData {
    pub(crate) fn read(packages: &ModePackages) -> Self {
        let mut data = Self::default();
        for view in packages.packages() {
            data.read_package(view.index, &view.package.files);
        }
        let mode = &packages.mode().package.files;
        data.read_map(mode, packages.map_name(), packages.heights().cloned());
        data.metric = packages.map().metric();
        data
    }

    /// The drawn heights a sim position stands above: the ground's on a planar map, where a
    /// position's height is above its ground; none on a spatial map, where a position is a point
    /// in space (04-capabilities/00-overview.md, The sim's ground is the map's height grid).
    pub(crate) const fn lifts(&self) -> Option<&GroundHeights> {
        match self.metric {
            Metric::Planar => self.heights.as_ref(),
            Metric::Spatial => None,
        }
    }

    /// Reads the mode's own package's files of `map`: its terrain over `grid`, its heights, which
    /// the packages read as they load, the camera, the map's lights and its client objects; and its
    /// conditions, by which each look takes its variant.
    fn read_map(&mut self, mode: &PackageReader, map: &MapName, grid: Option<HeightGrid>) {
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
        self.camera = Self::toml(mode, &CameraFile::path());
        self.hud = Self::toml(mode, &HudFile::path()).unwrap_or_default();
        self.team_colors = Self::toml(mode, &TeamColors::path()).unwrap_or_default();
        self.lights = Self::toml(mode, &MapLights::path(map));
        self.objects = Self::toml(mode, &ClientObjects::path(map)).unwrap_or_default();
        // The map's conditions are fixed for the match, so each look takes its variant once.
        let conditions = Self::toml::<MapConditions>(mode, &MapConditions::path(map))
            .unwrap_or_default()
            .conditions;
        for package in &mut self.packages {
            let units = &mut package.units;
            for look in units.units.values_mut().chain(units.objects.values_mut()) {
                look.settle(&conditions);
            }
        }
    }

    fn read_package(&mut self, place: PackageIndex, files: &PackageReader) {
        debug_assert_eq!(
            usize::from(place.get()),
            self.packages.len(),
            "packages read in their places' order"
        );
        let units = Self::toml::<ClientUnits>(files, &ClientUnits::path());
        self.packages.push(PackageClient {
            units: units.unwrap_or_default(),
        });
    }

    /// The client data of the package at `place`.
    pub(crate) fn of(&self, place: PackageIndex) -> &PackageClient {
        &self.packages[usize::from(place.get())]
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

    /// The TOML file at `path`; none for a file the package lacks, and none, logged, for one that
    /// does not read.
    fn toml<T: DeserializeOwned>(files: &PackageReader, path: &PackagePath) -> Option<T> {
        let text = Self::text(path, Self::read_file(files, path)?)?;
        Self::logged(path, Toml::parse(&text))
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
impl ClientData {
    /// The client data of packages of the looks `units`, in their places' order, as a test of the
    /// looks draws.
    pub(crate) fn of_looks(units: impl IntoIterator<Item = ClientUnits>) -> Self {
        let packages = units.into_iter().map(|units| PackageClient { units });
        Self {
            packages: packages.collect(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests;
