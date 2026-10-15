//! The Zero Hour install check, on request: imports the install `GENERALSZH_DATA` names into the
//! directory its command line names, in place of the packages an earlier run wrote there once
//! the new ones are whole, and logs the fingerprints of the imported package and the rules
//! package, which a run on another OS must match. Then it checks that Tournament Desert
//! imports as a multiplayer game of the original spawns it: 270 × 340 height samples; of its 580
//! objects, once its 176 road ends are dropped, 142 placed units of 41 types and 438 client
//! objects of 22 looks, each kind's templates logged for review; and 28 markers. It checks its
//! terrain file's 269 × 339 ground quads, its overlays, and its texture's copies and border
//! wraps, each logged for review; that the import reports no cell of its terrain whose tile has
//! no texture; its 286 walls of its grown cliffs; and that it loads in a match of the imported
//! mode, where player 1 starts with America's command center at `Player_1_Start` and its dozer
//! clear of the center's box, and the dozer, ordered to `Player_2_Start`, gets there within
//! `DRIVE_TICKS`, in no wall's cell in any tick. Every other map the game lists loads in the
//! imported mode. It fails
//! when `GENERALSZH_DATA` names no install, so a machine with no copy never passes as one that
//! checked.
//!
//! Run it with `./drive.py zero-hour [out]`, which builds it with the `release-dev` profile and
//! writes to `~/zh-package` unless `out` names another directory.

use std::collections::BTreeMap;
use std::env;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use campfire_client_files::{ClientObjects, ClientUnits, HeightGrid, MapConditions, TerrainFile};
use campfire_codec::{Binary, Toml, TomlError};
use campfire_common::{ExitStatus, MapName};
use campfire_import::{GameVersion, ZeroHour};
use campfire_log::{ErrorReport, Logging};
use campfire_math::Num;
use campfire_package::{ContentError, PackageReader};
use campfire_package_disk::PackageDir;
use campfire_store::{DirEntries, DurableDir, EntryKind, ReadError};
use campfire_values::{DeclaredName, PackagePath};
use tracing::{error, info};

use crate::command_line::CommandLine;
use crate::error::CheckError;
use crate::failure::{Failure, LookCounts, TerrainDrawn};
use crate::start_drive::StartDrive;

mod command_line;
mod error;
mod failure;
mod start_drive;

/// The map the check plays.
const MAP: &str = "tournament_desert";

/// Tournament Desert's walls: its 48,689 cliff cells grown by one ring, merged row by row
/// (design 17, D3).
const WALLS: usize = 286;

/// The most ticks player 1's dozer takes from its start to player 2's on Tournament Desert, at 30
/// ticks a second, which the check pins: what it took, 57.2 s.
const DRIVE_TICKS: u64 = 1717;

/// The looks the import gives night or snow variants, of unit types and of client objects; the
/// unit types' looks of no models, of objects that draw nothing; and the maps of night and of
/// snow among the 150.
const LOOKS: LookCounts = LookCounts {
    varied: 382,
    empty: 273,
    night: 15,
    snowy: 17,
};

/// Tournament Desert's terrain: a ground quad for each of the 269 × 339 cells the game draws;
/// its overlays; a copy for each tile its classes' textures load; and a border wrap for each
/// class's block.
const TERRAIN: TerrainDrawn = TerrainDrawn {
    quads: 269 * 339,
    overlays: 19_041,
    copies: 300,
    wraps: 19,
};

fn main() -> ExitCode {
    let _log = Logging {
        terminal: "info",
        file: "info",
    }
    .start();
    let out = match Logging::command_line::<CommandLine>(env::args_os()) {
        Ok(line) => line.out,
        Err(status) => return ExitCode::from(status),
    };
    match run(&out) {
        Ok(failures) if failures.is_empty() => {
            info!("the Zero Hour check passed");
            ExitCode::from(ExitStatus::Success)
        }
        Ok(failures) => {
            for failure in &failures {
                error!(%failure, "a failure of the Zero Hour check");
            }
            error!(failures = failures.len(), "the Zero Hour check failed");
            ExitCode::from(ExitStatus::Failure)
        }
        Err(problem) => {
            error!(error = %ErrorReport::of(&problem), "the Zero Hour check did not run");
            ExitCode::from(ExitStatus::Failure)
        }
    }
}

/// Imports the install into `out`, in place of the packages an earlier run wrote there once the
/// new ones are whole, and checks Tournament Desert; the failures it found.
fn run(out: &Path) -> Result<Vec<Failure>, CheckError> {
    let install = env::var_os("GENERALSZH_DATA")
        .filter(|install| !install.is_empty())
        .map(PathBuf::from)
        .ok_or(CheckError::NoInstall)?;
    let zero_hour = ZeroHour::open(&install, GameVersion::KNOWN).map_err(CheckError::Import)?;
    let name = out
        .file_name()
        .ok_or_else(|| CheckError::NoName(out.to_owned()))?;
    let mut staged = name.to_owned();
    staged.push(".new");
    let stage = out.with_file_name(staged);
    clear(&stage)?;
    let imported = zero_hour.write(&stage).map_err(CheckError::Import)?;
    clear(out)?;
    DurableDir::rename(&stage, out).map_err(CheckError::Rename)?;
    info!(
        version = zero_hour.version().name,
        fingerprint = %imported.fingerprint,
        rules = %imported.rules,
        packages = %out.display(),
        "imported"
    );
    let map = MapName::new(MAP).expect("a map's name");
    let packages = PackageDir::new(out.join(ZeroHour::GAME))
        .mode_packages(&map)
        .map_err(CheckError::Load)?;
    let files = &packages.mode().package.files;
    let mut failures = Vec::new();
    let heights = read(files, &format!("map/{MAP}/heights.bin"))?;
    let grid = Binary::decode::<HeightGrid>(&heights).map_err(CheckError::Decode)?;
    if (grid.columns(), grid.rows()) != (270, 340) {
        failures.push(Failure::Heights {
            columns: grid.columns(),
            rows: grid.rows(),
        });
    }
    let placed = &packages.map().units;
    let units = placed.len();
    let by_type = tally(placed.iter().map(|unit| unit.unit_type.as_str()));
    let types = by_type.len();
    if (units, types) != (142, 41) {
        failures.push(Failure::Units { units, types });
    }
    let client: ClientObjects = read_toml(files, &ClientObjects::path(MAP), Toml::parse)?;
    let objects = client.objects.len();
    let by_look = tally(client.objects.iter().map(|object| object.look.as_str()));
    let looks = by_look.len();
    if (objects, looks) != (438, 22) {
        failures.push(Failure::ClientObjects { objects, looks });
    }
    info!(units = %listed(&by_type), "Tournament Desert's units by type");
    info!(objects = %listed(&by_look), "Tournament Desert's client objects by look");
    let markers = packages.map().markers.len();
    if markers != 28 {
        failures.push(Failure::Markers(markers));
    }
    // The looks that draw other models on a night or snowy map, those that draw nothing, and the
    // maps of night and of snow.
    let unit_looks: ClientUnits = read_toml(files, "client/units.toml", Toml::parse)?;
    let varied = unit_looks
        .units
        .values()
        .chain(unit_looks.objects.values())
        .filter(|look| !look.variants.is_empty())
        .count();
    let empty = unit_looks
        .units
        .values()
        .filter(|look| look.models.is_empty() && look.variants.is_empty())
        .count();
    let [night, snow] = ["night", "snow"].map(|name| DeclaredName::new(name).expect("a name"));
    let mut drawn = LookCounts {
        varied,
        empty,
        night: 0,
        snowy: 0,
    };
    for other in imported.untextured_cells.keys() {
        let held: MapConditions =
            read_toml(files, &MapConditions::path(other.as_str()), Toml::parse)?;
        drawn.night += usize::from(held.conditions.contains(&night));
        drawn.snowy += usize::from(held.conditions.contains(&snow));
    }
    if drawn != LOOKS {
        failures.push(Failure::Looks(drawn));
    }
    info!(
        varied = drawn.varied,
        empty = drawn.empty,
        night = drawn.night,
        snowy = drawn.snowy,
        "the looks of night and snow"
    );
    let terrain = read(files, &TerrainFile::path(MAP))?;
    let terrain = Binary::decode::<TerrainFile>(&terrain).map_err(CheckError::Decode)?;
    let drawn = TerrainDrawn {
        quads: terrain.quads().ground.len(),
        overlays: terrain.quads().overlays.len(),
        copies: terrain.texture().copies.len(),
        wraps: terrain.texture().wraps.len(),
    };
    if drawn != TERRAIN {
        failures.push(Failure::Terrain(drawn));
    }
    let untextured = imported.untextured_cells[&map];
    if untextured > 0 {
        failures.push(Failure::Untextured(untextured));
    }
    let navigation = packages.map().navigation.as_ref();
    let walls = navigation.map_or(0, |navigation| navigation.walls.len());
    if walls != WALLS {
        failures.push(Failure::Walls(walls));
    }
    info!(
        samples = %format!("{} × {}", grid.columns(), grid.rows()),
        units,
        types,
        objects,
        looks,
        markers,
        quads = drawn.quads,
        overlays = drawn.overlays,
        copies = drawn.copies,
        wraps = drawn.wraps,
        untextured,
        walls,
        "Tournament Desert loaded"
    );
    // Every other map the game lists loads in the imported mode, its walls and its placed bodies
    // checked as a match loads them.
    let game = PackageDir::new(out.join(ZeroHour::GAME));
    let mut loaded = 1;
    for other in imported
        .untextured_cells
        .keys()
        .filter(|&other| *other != map)
    {
        match game.mode_packages(other) {
            Ok(_) => loaded += 1,
            Err(error) => failures.push(Failure::MapLoad {
                map: other.clone(),
                error,
            }),
        }
    }
    info!(
        loaded,
        maps = imported.untextured_cells.len(),
        "the imported maps loaded"
    );
    for (map, waypoints) in &imported.dropped_waypoints {
        info!(
            %map,
            waypoints = %waypoints.join(", "),
            "a map's waypoints past its bounds are dropped"
        );
    }
    let rate = NonZeroU32::new(30).expect("a rate");
    if let Some(drive) = StartDrive::run(packages, rate, &mut failures) {
        if drive.ticks > DRIVE_TICKS {
            failures.push(Failure::DriveTicks(drive.ticks));
        }
        if drive.left != Num::ZERO {
            failures.push(Failure::DriveShort(drive.left));
        }
        info!(
            ticks = drive.ticks,
            end = %format!("({}, {})", drive.end.x, drive.end.z),
            left = %drive.left,
            "the dozer drove from player 1's start to player 2's"
        );
    }
    Ok(failures)
}

/// Removes the packages an earlier run wrote to `out`, so the import writes it anew; nothing for
/// no directory there, and an error for one that holds anything but the import's two packages,
/// which no run removes.
fn clear(out: &Path) -> Result<(), CheckError> {
    let entries = match DirEntries::read(out) {
        Ok(entries) => entries,
        Err(error) if matches!(error.error, ReadError::Missing) => return Ok(()),
        Err(error) => return Err(CheckError::List(error)),
    };
    let packages = entries.iter().all(|entry| {
        entry.kind == EntryKind::Dir
            && [ZeroHour::GAME, ZeroHour::RULES].contains(&entry.name.to_str().unwrap_or_default())
    });
    if !packages {
        return Err(CheckError::NotPackages(out.to_owned()));
    }
    DurableDir::remove(out).map_err(CheckError::Remove)
}

/// How many of `names` there are of each name.
fn tally<'a>(names: impl IntoIterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    let mut counts = BTreeMap::new();
    for name in names {
        *counts.entry(name).or_default() += 1;
    }
    counts
}

/// `counts` for a log: `name × count`, in the order of the names.
fn listed(counts: &BTreeMap<&str, usize>) -> String {
    let entries: Vec<String> = counts
        .iter()
        .map(|(name, count)| format!("{name} × {count}"))
        .collect();
    entries.join(", ")
}

/// The package's TOML file at `path`, as `parse` reads its text.
fn read_toml<T>(
    files: &PackageReader,
    path: &str,
    parse: fn(&str) -> Result<T, TomlError>,
) -> Result<T, CheckError> {
    let path = PackagePath::parse(path).expect("a map's file is a package path");
    let bytes = files.read_file(&path).map_err(CheckError::Read)?;
    let text = String::from_utf8(bytes).map_err(|error| {
        CheckError::Read(ContentError::NotText {
            path: path.clone(),
            error: error.utf8_error(),
        })
    })?;
    parse(&text).map_err(|error| CheckError::Read(ContentError::Data { path, error }))
}

/// The bytes of the package's file at `path`.
fn read(files: &PackageReader, path: &str) -> Result<Vec<u8>, CheckError> {
    let path = PackagePath::parse(path).expect("a map's file is a package path");
    files.read_file(&path).map_err(CheckError::Read)
}
