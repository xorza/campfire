//! The Zero Hour install check, on request: imports the install `GENERALSZH_DATA` names, checks
//! the import and its matches against a closed form of the game's data and against `golden.toml`
//! (09-zero-hour.md, The check's measures are a golden file), and fails when it names no install.
//!
//! Run it with `cargo drive run zero-hour [out]`, which builds it with the `checks` profile;
//! it writes to `~/zh-package` unless `out` names another directory. `CAMPFIRE_BLESS=1` before
//! it writes the golden file again of what the run measured.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use campfire_common::ErrorReport;
use campfire_common::{ExitStatus, MapName};
use campfire_import::{Imported, ZeroHour};
use campfire_logging::Logging;
use campfire_package::ModePackages;
use campfire_package_disk::PackageDir;
use campfire_store::{DirEntries, DurableDir, EntryKind, ReadError};
use tracing::{error, info};

use crate::command_line::CommandLine;
use crate::error::CheckError;
use crate::failure::CheckFailure;
use crate::golden::{Golden, Measured};
use crate::import_check::ImportCheck;
use crate::pins::{Fingerprints, MAP};

mod attack_play;
mod base_play;
mod command_line;
mod defense_play;
mod error;
mod failure;
mod golden;
mod import_check;
mod match_play;
mod missile_play;
mod pins;
mod skirmish_play;
mod start_drive;
mod supply_play;

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
        packages: ModePackages::at(&game, Some(&map)).map_err(CheckError::Load)?,
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
    check.supply();
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
