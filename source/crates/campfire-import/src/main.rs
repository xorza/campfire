//! Imports a game's install into a package: `campfire-import <GAME> <INSTALL> <OUT>`. It reads
//! the install as the game reads it, refuses a version it does not know, and writes the package
//! into the new directory `OUT`, then logs its fingerprint. It logs as `campfire_log::Logging`
//! says, `info` by default.

use std::env;
use std::process::ExitCode;

use campfire_common::ExitStatus;
use campfire_import::{GameVersion, Imported, SkipReason, ZeroHour};
use campfire_log::{ErrorReport, Logging};
use tracing::{error, info, warn};

use crate::args::{Args, Game};

mod args;

fn main() -> ExitCode {
    let _log = Logging {
        terminal: "info",
        file: "info",
    }
    .start();
    let args: Args = match Logging::command_line(env::args_os()) {
        Ok(args) => args,
        Err(status) => return ExitCode::from(status),
    };
    let imported = match args.game {
        Game::ZeroHour => ZeroHour::open(&args.install, GameVersion::KNOWN).and_then(|mut game| {
            info!(
                version = game.version().name,
                "the install is a version the importer knows"
            );
            game.write(&args.out)
        }),
    };
    match imported {
        Ok(Imported {
            fingerprint,
            skipped_models,
            missing_textures,
        }) => {
            for skipped in &skipped_models {
                let model = &skipped.name;
                match skipped.reason {
                    SkipReason::Missing => warn!(
                        model,
                        "an object names a model no file of the install holds"
                    ),
                    SkipReason::Emitter => warn!(
                        model,
                        "an object names a particle emitter, which the importer does not convert"
                    ),
                }
            }
            for texture in &missing_textures {
                warn!(
                    texture,
                    "a model names a texture no archive holds, and draws untextured"
                );
            }
            info!(
                package = %args.out.display(),
                %fingerprint,
                skipped_models = skipped_models.len(),
                missing_textures = missing_textures.len(),
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
