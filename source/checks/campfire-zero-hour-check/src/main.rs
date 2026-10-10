//! The Zero Hour install check, on request: imports the install `GENERALSZH_DATA` names into a
//! new directory below the run root and logs the fingerprints of the imported package and the
//! rules package, which a run on another OS must match. Then it checks that Tournament Desert
//! imports as the original holds it: 270 × 340 height samples, 580 placed units of 63 types, and
//! 28 markers, once its 176 road ends are dropped; that every cell of its terrain the client draws
//! has a texture for each tile it draws; and that it loads in a match of the imported mode, which
//! plays a tick. It fails when `GENERALSZH_DATA` names no install, so a machine with no copy never
//! passes as one that checked.
//!
//! Run it with `cargo run --release -p campfire-zero-hour-check [-- <run root>]`.

use std::collections::BTreeSet;
use std::env;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use campfire_capabilities::{HeightGrid, PackagePath};
use campfire_common::{Binary, ExitStatus, MapName, Toml};
use campfire_import::{GameVersion, ZeroHour};
use campfire_log::{ErrorReport, Logging};
use campfire_package::{
    ClassTexture, ContentError, GameData, ModePackages, PackageReader, Terrain, TerrainAtlas,
};
use campfire_runner::internals::FixedSession;
use campfire_store::DurableFile;
use tracing::{error, info};

use crate::command_line::CommandLine;
use crate::error::CheckError;
use crate::failure::Failure;

mod command_line;
mod error;
mod failure;

/// The map the check plays.
const MAP: &str = "tournament_desert";

fn main() -> ExitCode {
    let _log = Logging {
        terminal: "info",
        file: "info",
    }
    .start();
    let root = match Logging::command_line::<CommandLine>(env::args_os()) {
        Ok(line) => line.root(),
        Err(status) => return ExitCode::from(status),
    };
    match run(&root) {
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

/// Imports the install into a new run directory below `root`, and checks Tournament Desert; the
/// failures it found.
fn run(root: &Path) -> Result<Vec<Failure>, CheckError> {
    let install = env::var_os("GENERALSZH_DATA")
        .filter(|install| !install.is_empty())
        .map(PathBuf::from)
        .ok_or(CheckError::NoInstall)?;
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970");
    DurableFile::create_dir_all(root).map_err(CheckError::RunRoot)?;
    let dir = root.join(format!("run-{}", since.as_millis()));
    DurableFile::create_new_dir(&dir).map_err(CheckError::RunDir)?;
    let out = dir.join("packages");
    let game = ZeroHour::open(&install, GameVersion::KNOWN).map_err(CheckError::Import)?;
    let imported = game.write(&out).map_err(CheckError::Import)?;
    info!(
        version = game.version().name,
        fingerprint = %imported.fingerprint,
        rules = %imported.rules,
        packages = %out.display(),
        "imported"
    );
    let map = MapName::new(MAP).expect("a map's name");
    let packages =
        ModePackages::from_dir(&out.join(ZeroHour::GAME), &map).map_err(CheckError::Load)?;
    let files = &packages
        .packages()
        .next()
        .expect("a mode has its own package")
        .package
        .files;
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
    let types = placed
        .iter()
        .map(|unit| &unit.unit_type)
        .collect::<BTreeSet<_>>()
        .len();
    if (units, types) != (580, 63) {
        failures.push(Failure::Units { units, types });
    }
    let markers = packages.map().markers.len();
    if markers != 28 {
        failures.push(Failure::Markers(markers));
    }
    let untextured = untextured(files)?;
    if untextured > 0 {
        failures.push(Failure::Untextured(untextured));
    }
    info!(
        samples = %format!("{} × {}", grid.columns(), grid.rows()),
        units,
        types,
        markers,
        untextured,
        "Tournament Desert loaded"
    );
    let rate = NonZeroU32::new(30).expect("a rate");
    // A fixed match takes the log's events while it lives, as its harness checks them, so it
    // ends before the check logs again.
    let tick = {
        let mut fixed = FixedSession::new(packages, rate, 1).start();
        fixed.runner_mut().run_tick();
        fixed.end().tick
    };
    info!(%tick, "Tournament Desert played a tick");
    Ok(failures)
}

/// How many cells of the map's terrain that the client draws, all but its last row and column,
/// draw a tile, a blend's or a third blend's when the game draws them, with no texture in the
/// atlas the client builds: a class with no texture, or whose texture holds too few tiles, or
/// that the atlas has no room for. A texture that does not read fails the check.
fn untextured(files: &PackageReader) -> Result<usize, CheckError> {
    let bytes = read(files, &format!("client/maps/{MAP}/terrain.bin"))?;
    let terrain = Binary::decode::<Terrain>(&bytes).map_err(CheckError::Decode)?;
    let path = PackagePath::parse(GameData::PATH).expect("a constant package path");
    let text = files.read_file(&path).map_err(CheckError::Read)?;
    let text = String::from_utf8(text).map_err(|error| {
        CheckError::Read(ContentError::NotText {
            path: path.clone(),
            error: error.utf8_error(),
        })
    })?;
    let game = Toml::parse::<GameData>(&text)
        .map_err(|error| CheckError::Read(ContentError::Data { path, error }))?;
    let parts = terrain.parts();
    let mut textures = Vec::new();
    for class in &parts.classes {
        let Some(path) = &class.texture else {
            textures.push(None);
            continue;
        };
        let bytes = files.read_file(path).map_err(CheckError::Read)?;
        let texture = ClassTexture::of_ktx2(&bytes).map_err(|error| CheckError::Texture {
            path: path.clone(),
            error,
        })?;
        textures.push(Some(texture));
    }
    let atlas = TerrainAtlas::new(parts, &textures);
    let columns = parts.columns as usize;
    let rows = parts.cells.len() / columns;
    let blend = |index: Option<u16>| index.map(|index| parts.blends[usize::from(index)].tile);
    // The package's rows run from the game's north, so the game's last row is its first.
    let drawn =
        (1..rows).flat_map(|row| (0..columns - 1).map(move |column| row * columns + column));
    Ok(drawn
        .filter(|&at| {
            let cell = &parts.cells[at];
            let extra = blend(cell.extra_blend).filter(|_| game.three_way_blends);
            [Some(cell.tile), blend(cell.blend), extra]
                .into_iter()
                .flatten()
                .any(|tile| atlas.tile(usize::from(tile >> 2)).is_none())
        })
        .count())
}

/// The bytes of the package's file at `path`.
fn read(files: &PackageReader, path: &str) -> Result<Vec<u8>, CheckError> {
    let path = PackagePath::parse(path).expect("a map's file is a package path");
    files.read_file(&path).map_err(CheckError::Read)
}
