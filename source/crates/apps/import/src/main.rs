//! Imports a game's install into a package: `campfire-import <GAME> <INSTALL> <OUT>`. It reads
//! the install as the game reads it, refuses a version it does not know, and writes into the new
//! directory `OUT` the game's rules package and the install's package, `game/`, the mode a player
//! plays, then logs their fingerprints. It logs as `campfire_logging::Logging`
//! says, `info` by default.

use std::env;
use std::process::ExitCode;

use campfire_common::ErrorReport;
use campfire_common::ExitStatus;
use campfire_import::{Imported, ZeroHour};
use campfire_logging::Logging;
use tracing::{error, info, warn};

use crate::command_line::{CommandLine, Game};

mod command_line;

fn main() -> ExitCode {
    let _log = Logging {
        terminal: "info",
        file: "info",
    }
    .start();
    let args: CommandLine = match Logging::command_line(env::args_os()) {
        Ok(args) => args,
        Err(status) => return ExitCode::from(status),
    };
    let imported = match args.game {
        Game::ZeroHour => ZeroHour::open(&args.install).and_then(|game| {
            info!(
                version = game.version().name(),
                "the install is a version the importer knows"
            );
            game.write(&args.out)
        }),
    };
    match imported {
        Ok(Imported {
            fingerprint,
            rules,
            maps,
            losses,
            ..
        }) => {
            for loss in &losses {
                warn!("{loss}");
            }
            info!(
                packages = %args.out.display(),
                %fingerprint,
                %rules,
                maps = maps.len(),
                losses = losses.len(),
                "imported"
            );
            ExitCode::from(ExitStatus::Success)
        }
        Err(error) => {
            error!(install = %args.install.display(), error = %ErrorReport::of(&error), "the install does not import");
            ExitCode::from(ExitStatus::Failure)
        }
    }
}
