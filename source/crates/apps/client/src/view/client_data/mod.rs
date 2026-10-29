use std::error::Error;
use std::sync::Arc;

use bevy::app::{App, Plugin, PostStartup};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::Commands;
use campfire_client_files::{
    CameraFile, ClientFile, ClientObjects, ClientUnits, EffectsFile, FogFile, HudFile, KeysFile,
    MapConditions, MapLights, PerksFile, PickingFile, SoundFile, TeamColors, TerrainFile,
};
use campfire_common::{DeclaredName, ErrorReport, MapName};
use campfire_package::ModePackages;
use campfire_package_format::{ContentError, PackageReader};
use campfire_units::HeightGrid;
use campfire_values::{PackageIndex, PackagePath};
use tracing::error;

use crate::client_packages::ClientPackages;
use crate::input_map::InputMap;
use crate::view::ground_heights::GroundHeights;
use crate::view::map_metric::MapMetric;
use crate::view::match_colors::MatchColors;
use crate::view::match_hud::MatchHud;
use crate::view::match_picking::MatchPicking;

/// The client data of the mode's packages that the client's start alone reads, which then goes:
/// each package's unit looks, by the package's place; the mode's own package's camera,
/// `client/camera.toml`; the map's lights, `client/maps/<map>/lights.toml`, its client objects,
/// `client/maps/<map>/objects.toml`, whose looks are its package's, and its terrain,
/// `client/maps/<map>/terrain.bin`, kept when its quads are the heights' cells. Each file is read
/// once, as the view starts, and checked against its package's index; one that does not read is
/// logged, and drawn as if it were not there.
#[derive(Resource, Debug, Default)]
pub(crate) struct ClientLoad {
    pub(crate) packages: Vec<PackageClient>,
    /// The map's terrain over its heights; none for a map of no heights, of no terrain file, or
    /// of one that does not read or is not the heights' shape.
    pub(crate) terrain: Option<TerrainFile>,
    /// The mode's own package's camera file.
    pub(crate) camera: Option<CameraFile>,
    /// The mode's own package's effects, `client/effects.toml`, none for a package of none.
    pub(crate) effects: Option<EffectsFile>,
    /// The mode's own package's sound, `client/sound.toml`, its default for a package of none.
    pub(crate) sound: SoundFile,
    /// The mode's own package's fog, `client/fog.toml`, its default for a package of none.
    pub(crate) fog: FogFile,
    /// The map's lights, which its units are drawn in.
    pub(crate) lights: Option<MapLights>,
    /// The map's client objects, none when it has no file of them.
    pub(crate) objects: ClientObjects,
    /// The map's conditions, which every unit's look holds for the whole match, none for a map of
    /// no file of them.
    pub(crate) map_conditions: Vec<DeclaredName>,
}

/// The client data of the mode's packages as the client reads it: what its start reads, and the
/// facts its frames read, each its own resource for the match: the ground's heights from the
/// map's heightmap, which the packages hold, none for a map of none; how the map reads a
/// position; and the mode's own package's HUD, `client/hud.toml`, picking,
/// `client/picking.toml`, and team colors, `client/team_colors.toml`, each its default for a
/// package of none.
#[derive(Debug)]
pub(crate) struct ClientFiles {
    pub(crate) load: ClientLoad,
    pub(crate) heights: Option<GroundHeights>,
    pub(crate) metric: MapMetric,
    pub(crate) hud: MatchHud,
    pub(crate) picking: MatchPicking,
    pub(crate) colors: MatchColors,
    pub(crate) perks: MatchPerks,
}

/// The mode's own package's perks, `client/perks.toml`, none for a package of none, which shows
/// no panel of perks.
#[derive(Resource, Debug, Default)]
pub(crate) struct MatchPerks(pub(crate) Option<PerksFile>);

/// A package's client data: its unit looks, `client/units.toml`.
#[derive(Debug, Default)]
pub(crate) struct PackageClient {
    pub(crate) units: ClientUnits,
}

/// Reads the mode's client data as the client starts: the one source of `ClientLoad`, which goes
/// once the start ran, of the facts the frames read, of `ClientPackages`, and of the `InputMap`,
/// the mode's own package's `client/keys.toml`.
#[derive(Debug)]
pub(crate) struct ClientFilesPlugin {
    pub(crate) packages: Arc<ModePackages>,
}

impl Plugin for ClientFilesPlugin {
    fn build(&self, app: &mut App) {
        let ClientFiles {
            load,
            heights,
            metric,
            hud,
            picking,
            colors,
            perks,
        } = ClientLoad::read(&self.packages);
        app.insert_resource(load);
        if let Some(heights) = heights {
            app.insert_resource(heights);
        }
        app.insert_resource(metric);
        app.insert_resource(hud);
        app.insert_resource(picking);
        app.insert_resource(colors);
        app.insert_resource(perks);
        app.add_systems(PostStartup, ClientLoad::drop);
        app.insert_resource(ClientPackages::of(&self.packages));
        let mode = &self.packages.mode().package.files;
        let input = ClientLoad::file::<KeysFile>(mode, &KeysFile::path()).unwrap_or_default();
        app.insert_resource(InputMap::new(input));
    }
}

impl ClientLoad {
    pub(crate) fn read(packages: &ModePackages) -> ClientFiles {
        let mut load = Self::default();
        for view in packages.packages() {
            load.read_package(view.index, &view.package.files);
        }
        let mode = &packages.mode().package.files;
        let map = packages.map_name();
        let heights = load.read_ground(mode, map, packages.heights().cloned());
        load.read_map(mode, map);
        ClientFiles {
            load,
            heights,
            metric: MapMetric(packages.map().metric()),
            hud: MatchHud(Self::file(mode, &HudFile::path()).unwrap_or_default()),
            picking: MatchPicking(Self::file(mode, &PickingFile::path()).unwrap_or_default()),
            colors: MatchColors::new(Self::file(mode, &TeamColors::path()).unwrap_or_default()),
            perks: MatchPerks(Self::file(mode, &PerksFile::path())),
        }
    }

    /// Reads the terrain of `map` over `grid`, the map's heights, which the packages read as they
    /// load, kept when its quads are the grid's cells; and gives the ground's heights, split as
    /// the terrain draws its cells, none for a map of no heights.
    fn read_ground(
        &mut self,
        mode: &PackageReader,
        map: &MapName,
        grid: Option<HeightGrid>,
    ) -> Option<GroundHeights> {
        let terrain = TerrainFile::path(map);
        self.terrain = grid.as_ref().and_then(|grid| {
            let file = Self::file::<TerrainFile>(mode, &terrain)?;
            let fits = file.quads().fits(grid.columns(), grid.rows());
            if !fits {
                error!(path = %terrain, "a map's terrain is not the shape of its heights");
            }
            fits.then_some(file)
        });
        let quads = self.terrain.as_ref().map(TerrainFile::quads);
        grid.map(|grid| GroundHeights::of(grid, quads))
    }

    /// Drops the start's files once the start ran: no frame reads them.
    pub(crate) fn drop(mut commands: Commands<'_, '_>) {
        commands.remove_resource::<Self>();
    }

    /// Reads the mode's own package's files of `map` that the start reads: the camera, the map's
    /// lights and its client objects; and its conditions, which each unit's look holds and by
    /// which each client object's look takes its variant.
    fn read_map(&mut self, mode: &PackageReader, map: &MapName) {
        self.camera = Self::file(mode, &CameraFile::path());
        self.effects = Self::file(mode, &EffectsFile::path());
        self.sound = Self::file(mode, &SoundFile::path()).unwrap_or_default();
        self.fog = Self::file(mode, &FogFile::path()).unwrap_or_default();
        self.lights = Self::file(mode, &MapLights::path(map));
        self.objects = Self::file(mode, &ClientObjects::path(map)).unwrap_or_default();
        // The map's conditions are fixed for the match, so each object's look takes its variant
        // once.
        let conditions = Self::file::<MapConditions>(mode, &MapConditions::path(map))
            .unwrap_or_default()
            .conditions;
        for package in &mut self.packages {
            package.units.settle_objects(&conditions);
        }
        self.map_conditions = conditions;
    }

    fn read_package(&mut self, place: PackageIndex, files: &PackageReader) {
        debug_assert_eq!(
            usize::from(place.get()),
            self.packages.len(),
            "packages read in their places' order"
        );
        let units = Self::file::<ClientUnits>(files, &ClientUnits::path());
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

    /// The client file at `path`; none for a file the package lacks, and none, logged, for one
    /// that does not read.
    fn file<T: ClientFile>(files: &PackageReader, path: &PackagePath) -> Option<T> {
        let bytes = Self::read_file(files, path)?;
        Self::logged(path, T::decode(&bytes))
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
pub(crate) mod internals {
    use campfire_client_files::ClientUnits;

    use crate::view::client_data::{ClientLoad, PackageClient};

    impl ClientLoad {
        /// The client data of packages of the looks `units`, in their places' order, as a test
        /// of the looks draws.
        pub(crate) fn of_looks(units: impl IntoIterator<Item = ClientUnits>) -> Self {
            let packages = units.into_iter().map(|units| PackageClient { units });
            Self {
                packages: packages.collect(),
                ..Self::default()
            }
        }
    }
}

#[cfg(test)]
mod tests;
