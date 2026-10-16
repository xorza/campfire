//! Imports a game's install into a package: `campfire-import <GAME> <INSTALL> <OUT>`. It reads
//! the install as the game reads it, refuses a version it does not know, and writes into the new
//! directory `OUT` the game's rules package and the install's package, `game/`, the mode a player
//! plays, then logs their fingerprints. It logs as `campfire_log::Logging`
//! says, `info` by default.

use std::env;
use std::process::ExitCode;

use campfire_common::ExitStatus;
use campfire_import::{Imported, SkipReason, ZeroHour};
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
            skipped_models,
            missing_textures,
            maps,
            dropped_buttons,
            dropped_weapons,
            unliving,
            wounded,
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
                    SkipReason::Aggregate => warn!(
                        model,
                        "a model's HLOD adds an aggregate, which the importer does not convert"
                    ),
                }
            }
            for texture in &missing_textures {
                warn!(
                    texture,
                    "a model names a texture no archive holds, and draws untextured"
                );
            }
            for (map, imported) in &maps {
                let cells = imported.untextured_cells;
                if cells > 0 {
                    warn!(%map, cells, "a map's terrain draws cells of a tile with no texture");
                }
                if !imported.dropped_waypoints.is_empty() {
                    warn!(
                        %map,
                        waypoints = %imported.dropped_waypoints.join(", "),
                        "a map's waypoints past its bounds are dropped"
                    );
                }
            }
            for dropped in &dropped_buttons {
                warn!(
                    button = dropped.button,
                    cause = ?dropped.cause,
                    "a command button is not imported"
                );
            }
            for dropped in &dropped_weapons {
                warn!(
                    weapon = dropped.weapon,
                    cause = ?dropped.cause,
                    "a weapon is not imported"
                );
            }
            for object in &unliving {
                warn!(object, "an object's body is none the import gives life");
            }
            for object in &wounded {
                warn!(
                    object,
                    "an object's body starts below its most life, which the engine starts full"
                );
            }
            info!(
                packages = %args.out.display(),
                %fingerprint,
                %rules,
                skipped_models = skipped_models.len(),
                missing_textures = missing_textures.len(),
                maps = maps.len(),
                untextured_cells = maps.values().map(|map| map.untextured_cells).sum::<usize>(),
                dropped_buttons = dropped_buttons.len(),
                dropped_weapons = dropped_weapons.len(),
                unliving = unliving.len(),
                wounded = wounded.len(),
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
