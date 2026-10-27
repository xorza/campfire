use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

use campfire_console::{ConsoleOpening, Output};
use campfire_store::InputFile;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::info;

use crate::error::GameFailure;
use crate::game_kind::{BuildProfile, GameKind};
use crate::game_process::{GameProcess, StartedGame};
use crate::process_exit::ProcessExit;

/// The games this server started, each by its handle, and where their builds are: the
/// workspace's source directory, which the driver builds in, and its target directory
/// (console.md, The MCP server never builds itself).
#[derive(Debug)]
pub(crate) struct Games {
    source: PathBuf,
    target: PathBuf,
    /// This server's own binary, and its time of change as it started, which a newer build moves.
    own: Option<OwnBinary>,
    games: Mutex<BTreeMap<u32, Arc<GameProcess>>>,
    next: AtomicU32,
}

/// This server's binary, and its time of change as the server started.
#[derive(Debug)]
struct OwnBinary {
    path: PathBuf,
    modified: SystemTime,
}

/// A game a call started: its handle, its opening line, and whether this server's own binary is
/// newer than its process.
#[derive(Debug)]
pub(crate) struct Launched {
    pub(crate) handle: u32,
    pub(crate) opening: ConsoleOpening,
    pub(crate) own_newer: bool,
}

impl Games {
    /// How long a stop waits for a game whose input closed before it ends it.
    pub(crate) const PATIENCE: Duration = Duration::from_secs(5);

    /// The last lines of a failed build a failure names.
    const BUILD_TAIL: usize = 20;

    /// Games of the workspace whose target directory is `target`, in its parent, of this
    /// server's binary at `own`, when it knows it.
    pub(crate) fn new(target: PathBuf, own: Option<PathBuf>) -> Self {
        let source = target
            .parent()
            .expect("a target directory lies in the workspace")
            .to_owned();
        let own = own.and_then(|path| {
            let modified = InputFile::stream(&path).ok()?.stamp().modified;
            Some(OwnBinary { path, modified })
        });
        Self {
            source,
            target,
            own,
            games: Mutex::new(BTreeMap::new()),
            next: AtomicU32::new(1),
        }
    }

    /// Builds the games in `profile` through the driver, its output into the log, then starts
    /// `kind` with `args` and its console; refuses a game of another console's fingerprint.
    pub(crate) async fn start(
        &self,
        kind: GameKind,
        args: &[OsString],
        profile: BuildProfile,
    ) -> Result<Launched, GameFailure> {
        self.build(profile).await?;
        self.launch(&kind.binary(&self.target, profile), args).await
    }

    /// Starts the game `binary` with `args` and its console, as `start` does once it built.
    pub(crate) async fn launch(
        &self,
        binary: &Path,
        args: &[OsString],
    ) -> Result<Launched, GameFailure> {
        let StartedGame { game, opening } = GameProcess::start(binary, args).await?;
        let own = Output::fingerprint();
        if opening.fingerprint != own {
            game.stop(Self::PATIENCE).await;
            return Err(GameFailure::Fingerprint {
                game: opening.fingerprint,
                server: own,
            });
        }
        let handle = self.next.fetch_add(1, Ordering::Relaxed);
        self.games.lock().await.insert(handle, Arc::new(game));
        let own_newer = self.own.as_ref().is_some_and(|own| {
            let now = InputFile::stream(&own.path).map(|file| file.stamp().modified);
            now.is_ok_and(|now| now > own.modified)
        });
        Ok(Launched {
            handle,
            opening,
            own_newer,
        })
    }

    /// The game of `handle`, or the only one with none.
    pub(crate) async fn get(&self, handle: Option<u32>) -> Result<Arc<GameProcess>, GameFailure> {
        let games = self.games.lock().await;
        let handles = || games.keys().copied().collect::<Vec<u32>>();
        match handle {
            Some(handle) => games
                .get(&handle)
                .cloned()
                .ok_or_else(|| GameFailure::UnknownHandle {
                    handle,
                    games: handles(),
                }),
            None => match games.len() {
                0 => Err(GameFailure::NoGame),
                1 => Ok(Arc::clone(games.values().next().expect("one game"))),
                _ => Err(GameFailure::Several { games: handles() }),
            },
        }
    }

    /// Stops the game of `handle`, or the only one, and gives its handle and exit status.
    pub(crate) async fn stop(&self, handle: Option<u32>) -> Result<GameStopped, GameFailure> {
        let game = self.get(handle).await?;
        let mut games = self.games.lock().await;
        let handle = *games
            .iter()
            .find(|(_, held)| Arc::ptr_eq(held, &game))
            .map(|(handle, _)| handle)
            .expect("the game it found");
        games.remove(&handle);
        drop(games);
        let status = game.stop(Self::PATIENCE).await;
        Ok(GameStopped { handle, status })
    }

    /// Stops every game, as this server stops (console.md, A game stops as its console's input
    /// ends).
    pub(crate) async fn stop_all(&self) {
        let games = std::mem::take(&mut *self.games.lock().await);
        for game in games.into_values() {
            game.stop(Self::PATIENCE).await;
        }
    }

    /// Builds both games in `profile` through the driver's command, its output into the log; a
    /// build that fails fails with its last lines.
    async fn build(&self, profile: BuildProfile) -> Result<(), GameFailure> {
        let output = Command::new("cargo")
            .args(["drive", "run", profile.command()])
            .current_dir(&self.source)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(GameFailure::Driver)?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        for line in text.lines() {
            info!(line, "the build of the games");
        }
        if output.status.success() {
            return Ok(());
        }
        let lines: Vec<&str> = text.lines().collect();
        let tail = lines[lines.len().saturating_sub(Self::BUILD_TAIL)..].join("\n");
        Err(GameFailure::Build {
            status: output.status.into(),
            tail,
        })
    }
}

/// A game a stop ended: its handle and its exit status.
#[derive(Debug)]
pub(crate) struct GameStopped {
    pub(crate) handle: u32,
    pub(crate) status: ProcessExit,
}
