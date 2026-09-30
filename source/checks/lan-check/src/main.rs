//! LAN check on request: builds and starts the real `campfire-server` and two
//! `campfire-client --bot` processes over WebTransport on `127.0.0.1`, for a match of about 3 s
//! of the test lane mode; then reads their JSON logs and runs `campfire-verifier` on the session
//! log. It passes when every process succeeded and logged no warning or error, every order was
//! logged and took effect in its stamp tick, and the verifier gives the server's final hash.
//!
//! Run it with `cargo run -p campfire-lan-check`. The logs stay in the run's directory, which it
//! names.

use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use campfire_log::Logging;
use campfire_net::{InputLogged, Listening, MatchStarted, OrderScript, OrdersSent, SessionWritten};
use campfire_verifier::Verified;
use tracing::{error, info};

use crate::binaries::Binaries;
use crate::error::CheckError;
use crate::lan_match::LanMatch;
use crate::outcome::Outcome;
use crate::process::Process;
use crate::process_log::ProcessLog;
use crate::verdict::{BotEvents, Verdict};

mod binaries;
mod error;
mod failure;
mod lan_match;
mod outcome;
mod process;
mod process_log;
mod target_name;
mod verdict;

/// The test lane mode: a hero a side, 30 ticks a second.
const MODE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/test/modes/lane"
);
/// The packages the verifier holds, the lane mode's among them.
const PACKAGES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/test");
/// Each bot's orders: two steps near its spawn, then it leaves after tick 90, 3 s into the match.
const SCRIPTS: [&str; 2] = [
    "end = 90\n[[order]]\ntick = 20\nmove = [2, 0]\n[[order]]\ntick = 50\nmove = [-1, 2]\n",
    "end = 90\n[[order]]\ntick = 20\nmove = [-2, 0]\n[[order]]\ntick = 50\nmove = [1, -2]\n",
];

fn main() -> ExitCode {
    Logging {
        terminal: "info",
        file: "info",
    }
    .start();
    match check() {
        Ok(verdict) if verdict.failures().is_empty() => {
            info!("the LAN check passed");
            ExitCode::SUCCESS
        }
        Ok(verdict) => {
            for failure in verdict.failures() {
                error!(%failure, "the LAN check failed");
            }
            ExitCode::FAILURE
        }
        Err(error) => {
            error!(%error, "the LAN check did not run");
            ExitCode::FAILURE
        }
    }
}

fn check() -> Result<Verdict, CheckError> {
    let cargo = env::var_os("CARGO").ok_or(CheckError::NotUnderCargo)?;
    let binaries = Binaries::build(&cargo)?;
    let dir = env::temp_dir().join("campfire-lan-check");
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|error| CheckError::File {
            path: dir.clone(),
            error,
        })?;
    }
    fs::create_dir_all(&dir).map_err(|error| CheckError::File {
        path: dir.clone(),
        error,
    })?;
    info!(dir = %dir.display(), "the run's logs go here");
    let mut scripts = Vec::with_capacity(SCRIPTS.len());
    let mut scripted = Vec::with_capacity(SCRIPTS.len());
    for (index, text) in SCRIPTS.iter().enumerate() {
        scripted.push(
            OrderScript::parse(text)
                .map_err(CheckError::Script)?
                .orders()
                .len(),
        );
        let path = dir.join(format!("{}.toml", Process::Bot(index).file_stem()));
        fs::write(&path, text).map_err(|error| CheckError::File {
            path: path.clone(),
            error,
        })?;
        scripts.push(path);
    }

    let lan = LanMatch {
        binaries: &binaries,
        dir: &dir,
        mode: Path::new(MODE),
        scripts: &scripts,
    };
    let played = lan.play()?;
    let mut verdict = Verdict::default();
    let server = ProcessLog::read(Process::Server, &lan.log_path(Process::Server))?;
    verdict.process(Process::Server, played.server, &server);
    verdict.listened(&server.read_all::<Listening>()?);
    let mut bots = Vec::with_capacity(played.bots.len());
    for ((index, &outcome), &scripted) in played.bots.iter().enumerate().zip(&scripted) {
        let process = Process::Bot(index);
        let log = ProcessLog::read(process, &lan.log_path(process))?;
        verdict.process(process, outcome, &log);
        bots.push(BotEvents {
            started: log.first::<MatchStarted>()?,
            sent: log.read_all::<OrdersSent>()?,
            scripted,
        });
    }
    verdict.orders(&server.read_all::<InputLogged>()?, &bots);

    let written = server.first::<SessionWritten>()?;
    let verified = match &written {
        Some(written) => {
            let path = lan.log_path(Process::Verifier);
            let status = Command::new(&binaries.verifier)
                .arg(PACKAGES)
                .arg(&written.file)
                .current_dir(&dir)
                .env("CAMPFIRE_LOG", &path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|error| CheckError::Start {
                    process: Process::Verifier,
                    error,
                })?;
            let log = ProcessLog::read(Process::Verifier, &path)?;
            verdict.process(Process::Verifier, Outcome::of(status), &log);
            log.first::<Verified>()?
        }
        None => None,
    };
    verdict.hash(written.as_ref(), verified.as_ref());
    Ok(verdict)
}
