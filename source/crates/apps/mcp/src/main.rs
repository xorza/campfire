//! Campfire's MCP server: Claude Code starts it from the repository's `.mcp.json`, and it serves
//! the tools that start the workspace's games, each with its console, and read and drive their
//! matches, over MCP's stdio on a runtime of one thread (02-engine-core.md, One MCP server starts
//! the games). It finds the workspace by its own binary, built in the target directory's
//! `release-dev`. Its log goes to standard error, filtered by `RUST_LOG` (`info` by default).

use std::env;
use std::process::ExitCode;
use std::sync::Arc;

use campfire_common::{ErrorReport, ExitStatus};
use campfire_logging::Logging;
use rmcp::ServiceExt;
use rmcp::transport::stdio;
use tokio::runtime::Builder;
use tracing::error;

use crate::game_tools::GameTools;
use crate::games::Games;

mod error;
mod game_kind;
mod game_process;
mod game_tools;
mod games;
mod process_exit;
mod tool_name;
mod tool_params;

fn main() -> ExitCode {
    let _log = Logging {
        terminal: "info",
        file: "info",
    }
    .start();
    let own = env::current_exe().ok();
    // The binary lies in `<target>/<profile>/`.
    let Some(target) = own
        .as_ref()
        .and_then(|own| own.parent()?.parent())
        .map(ToOwned::to_owned)
    else {
        error!("the server's binary lies in no target directory");
        return ExitCode::from(ExitStatus::Failure);
    };
    let runtime = match Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "the runtime does not start");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let games = Arc::new(Games::new(target, own));
    runtime.block_on(async {
        let served = GameTools::new(Arc::clone(&games)).serve(stdio()).await;
        let status = match served {
            Ok(running) => match running.waiting().await {
                Ok(_) => ExitStatus::Success,
                Err(error) => {
                    error!(error = %ErrorReport::of(&error), "the server stopped");
                    ExitStatus::Failure
                }
            },
            Err(error) => {
                error!(error = %ErrorReport::of(&error), "the server does not start");
                ExitStatus::Failure
            }
        };
        games.stop_all().await;
        ExitCode::from(status)
    })
}
