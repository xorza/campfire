//! Imports a game's install into a package: `campfire-import <GAME> <INSTALL> <OUT>`. It reads
//! the install as the game reads it, refuses a version it does not know, and writes into the new
//! directory `OUT` the game's rules package and the install's package, `game/`, the mode a player
//! plays, then logs their fingerprints. It logs as `campfire_logging::Logging`
//! says, `info` by default.

use std::env;
use std::process::ExitCode;

use campfire_common::ErrorReport;
use campfire_common::ExitStatus;
use campfire_import::{ImportLoss, Imported, SkipReason, ZeroHour};
use campfire_logging::Logging;
use tracing::{error, info, warn};

use crate::command_line::{CommandLine, Game};

mod command_line;

/// Logs `loss` as a warning, with what it is and why.
fn warn_of(loss: &ImportLoss) {
    match loss {
        ImportLoss::Model(skipped) => {
            let model = &skipped.name;
            match skipped.reason {
                SkipReason::Missing => {
                    warn!(
                        model,
                        "an object names a model no file of the install holds"
                    );
                }
                SkipReason::Emitter => warn!(
                    model,
                    "an object names a particle emitter, which the importer does not convert"
                ),
                SkipReason::Aggregate => warn!(
                    model,
                    "a model's HLOD adds an aggregate, which the importer does not convert"
                ),
            }
        }
        ImportLoss::Texture(texture) => warn!(
            texture,
            "a model names a texture no archive holds, and draws the game's MissingTexture"
        ),
        ImportLoss::UntexturedCells { map, cells } => {
            warn!(%map, cells, "a map's terrain draws cells of a tile with no texture");
        }
        ImportLoss::Waypoint { map, waypoint } => {
            warn!(%map, waypoint, "a map's waypoint past its bounds is dropped");
        }
        ImportLoss::Button(dropped) => warn!(
            button = dropped.name,
            cause = ?dropped.cause,
            "a command button is not imported"
        ),
        ImportLoss::Supply(dropped) => warn!(
            object = dropped.name,
            cause = ?dropped.cause,
            "an object's supply is not imported"
        ),
        ImportLoss::Weapon(dropped) => warn!(
            weapon = dropped.name,
            cause = ?dropped.cause,
            "a weapon is not imported"
        ),
        ImportLoss::Unliving(object) => {
            warn!(object, "an object's body is none the import gives life");
        }
        ImportLoss::Wounded(object) => warn!(
            object,
            "an object's body starts below its most life, which the engine starts full"
        ),
    }
}

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
        }) => {
            for loss in &losses {
                warn_of(loss);
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
