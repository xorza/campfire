use std::io;

use campfire_codec::JsonError;
use campfire_console::{ConsoleRefusal, Fingerprint};
use thiserror::Error;

use crate::process_exit::ProcessExit;

/// Why a tool's call of a game fails, each case with what the caller should change; its text is
/// the call's result, which the agent reads.
#[derive(Debug, Error)]
pub(crate) enum GameFailure {
    #[error("no game runs; start one with game_start")]
    NoGame,
    #[error("no game has the handle {handle}; the games are {games:?}")]
    UnknownHandle { handle: u32, games: Vec<u32> },
    #[error("several games run, {games:?}; name one by its handle")]
    Several { games: Vec<u32> },
    #[error("the build failed with {status}; its last lines:\n{tail}")]
    Build { status: ProcessExit, tail: String },
    #[error("the game's binary does not start")]
    Spawn(#[source] io::Error),
    #[error("the driver, `cargo drive`, does not start to build the games")]
    Driver(#[source] io::Error),
    #[error(
        "the game's console is of other types, fingerprint {game}, and this server's {server}: \
         build both again, and restart this server (/mcp) when its own build is the older"
    )]
    Fingerprint {
        game: Fingerprint,
        server: Fingerprint,
    },
    #[error("the game exited, {status}; its last errors:\n{errors}")]
    Exited { status: ProcessExit, errors: String },
    #[error("the game refused the request: {0:?}")]
    Refused(ConsoleRefusal),
    #[error("the call's arguments do not read")]
    Arguments(#[source] JsonError),
    #[error("the call was cancelled, and the game's run ended where it stood")]
    Cancelled,
}
