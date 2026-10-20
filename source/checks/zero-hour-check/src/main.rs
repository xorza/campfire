//! The Zero Hour install check, on request: imports the install `GENERALSZH_DATA` names into the
//! directory its command line names, in place of the packages an earlier run wrote there once the
//! new ones are whole, and checks what the import gives against what it expects: what a closed
//! form of the game's data gives, and what it measured once, which `golden.toml` beside it pins
//! (09-zero-hour.md, The check's measures are a golden file). It checks the fingerprints of the
//! imported package and the rules package, which a run on Linux and a run on macOS give alike;
//! Tournament Desert's height samples, placed units and their types, client objects and their
//! looks, each kind's templates logged for review, and markers; its terrain file's ground quads,
//! overlays, and its texture's copies and border wraps; that the import reports no cell of its
//! terrain whose tile has no texture; its walls of its grown cliffs; and that it loads in a match
//! of the imported mode, where player 1 starts with America's command center at `Player_1_Start`
//! and its dozer clear of the center's box, and the dozer, ordered to `Player_2_Start`, gets
//! there in its pinned ticks, in no wall in any tick. It counts the trains and builds the import
//! converts from the command sets and the buttons it drops by cause; and in a match of its own,
//! player 1's command center trains a dozer, its dozer builds a barracks, and the barracks trains
//! a ranger, each in its ticks and paid from the 50,000 the host picks of the starting money;
//! then its dozer builds a power plant, a supply center and a war factory, the factory trains
//! three crusaders, and their shells destroy the bot's command center, in the ticks of the first
//! shot, the first damage and the center's death, and the match goes on, as the bot's other
//! buildings count for its victory. The bot plays the builder's orders, `bots/builder.toml`: it
//! builds its base, trains its units, each counted and pinned, and attacks nothing, its units no
//! farther from its center than the pinned distance. Every other map the game lists loads in the
//! imported mode. It fails when `GENERALSZH_DATA` names no install, so a machine with no copy
//! never passes as one that checked.
//!
//! Run it with `cargo drive run zero-hour [out]`, which builds it with the `release-dev` profile;
//! it writes to `~/zh-package` unless `out` names another directory. `CAMPFIRE_BLESS=1` before
//! it writes the golden file again of what the run measured.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use campfire_common::ErrorReport;
use campfire_common::{ExitStatus, MapName};
use campfire_import::{Imported, ZeroHour};
use campfire_log::Logging;
use campfire_package_disk::PackageDir;
use campfire_store::{DirEntries, DurableDir, EntryKind, ReadError};
use tracing::{error, info};

use crate::command_line::CommandLine;
use crate::error::CheckError;
use crate::failure::CheckFailure;
use crate::golden::{Golden, Measured};
use crate::import_check::ImportCheck;
use crate::pins::{Fingerprints, MAP};

mod command_line;
mod error;
mod failure;
mod golden;
mod import_check;
mod match_play;
mod pins;
mod skirmish_play;
mod start_drive;

fn main() -> ExitCode {
    let _log = Logging {
        terminal: "info",
        file: "info",
    }
    .start();
    let line = match Logging::command_line::<CommandLine>(env::args_os()) {
        Ok(line) => line,
        Err(status) => return ExitCode::from(status),
    };
    let Some(out) = line.out_dir(env::home_dir()) else {
        error!("no directory for the packages: the line names none, and there is no home");
        return ExitCode::from(ExitStatus::Usage);
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
/// new ones are whole, and checks Tournament Desert against the golden file, or, in a run that
/// blesses, writes the file again; the failures it found.
fn run(out: &Path) -> Result<Vec<CheckFailure>, CheckError> {
    let golden = if Golden::is_blessing() {
        None
    } else {
        Some(Golden::read()?)
    };
    let imported = import(out)?;
    let map = MapName::parse(MAP).expect("a map's name");
    let game = PackageDir::new(out.join(ZeroHour::GAME));
    let mut check = ImportCheck {
        imported: &imported,
        game: &game,
        map: &map,
        packages: game.mode_packages(Some(&map)).map_err(CheckError::Load)?,
        golden,
        measured: Measured::default(),
        failures: Vec::new(),
    };
    check.fingerprints(Fingerprints {
        game: imported.fingerprint,
        rules: imported.rules,
    });
    check.map()?;
    check.looks()?;
    check.other_maps();
    check.production();
    check.combat();
    check.plays()
}

/// Imports the install `GENERALSZH_DATA` names into a directory beside `out`, then puts it in
/// the place of `out`; what the import wrote.
fn import(out: &Path) -> Result<Imported, CheckError> {
    let install = env::var_os("GENERALSZH_DATA")
        .filter(|install| !install.is_empty())
        .map(PathBuf::from)
        .ok_or(CheckError::NoInstall)?;
    let zero_hour = ZeroHour::open(&install).map_err(CheckError::Import)?;
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
        version = zero_hour.version().name(),
        fingerprint = %imported.fingerprint,
        rules = %imported.rules,
        packages = %out.display(),
        "imported"
    );
    Ok(imported)
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
