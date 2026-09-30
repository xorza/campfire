//! LAN check on request: builds and starts the real `campfire-server` and two
//! `campfire-client --bot` processes over WebTransport on `127.0.0.1`, for a match of about 3 s
//! of the test lane mode; then reads their JSON logs and runs `campfire-verifier` on the session
//! log. It passes when every process succeeded and logged no warning or error, every order was
//! logged and took effect in its stamp tick, and the verifier gives the server's final hash.
//!
//! Run it with `cargo run -p campfire-lan-check [-- <run directory>]`. The logs and the session
//! log stay in the run's directory. `cargo run -p campfire-lan-check -- verify <run directory>`
//! verifies the session log of a run, perhaps from another machine, with this machine's verifier,
//! and compares the server's final hash.

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
use crate::mode::Mode;
use crate::outcome::Outcome;
use crate::process::Process;
use crate::process_log::ProcessLog;
use crate::verdict::{BotEvents, Verdict};

mod binaries;
mod error;
mod failure;
mod lan_match;
mod mode;
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
    let Some(mode) = Mode::parse(env::args_os().skip(1)) else {
        error!("usage: campfire-lan-check [<run directory>] | verify <run directory>");
        return ExitCode::from(2);
    };
    let result = match mode {
        Mode::Play { dir } => play(&dir),
        Mode::Verify { dir } => verify_run(&dir),
    };
    match result {
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

/// Plays a LAN match with its logs in `dir`, which it empties first, and checks it.
fn play(dir: &Path) -> Result<Verdict, CheckError> {
    let binaries = build()?;
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|error| CheckError::File {
            path: dir.to_owned(),
            error,
        })?;
    }
    fs::create_dir_all(dir).map_err(|error| CheckError::File {
        path: dir.to_owned(),
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
        dir,
        mode: Path::new(MODE),
        scripts: &scripts,
    };
    let played = lan.play()?;
    let mut verdict = Verdict::default();
    let server = ProcessLog::read(Process::Server, &Process::Server.log_path(dir))?;
    verdict.process(Process::Server, played.server, &server);
    verdict.listened(&server.read_all::<Listening>()?);
    let mut bots = Vec::with_capacity(played.bots.len());
    for ((index, &outcome), &scripted) in played.bots.iter().enumerate().zip(&scripted) {
        let process = Process::Bot(index);
        let log = ProcessLog::read(process, &process.log_path(dir))?;
        verdict.process(process, outcome, &log);
        bots.push(BotEvents {
            started: log.first::<MatchStarted>()?,
            sent: log.read_all::<OrdersSent>()?,
            scripted,
        });
    }
    verdict.orders(&server.read_all::<InputLogged>()?, &bots);

    verify(&binaries, dir, &server, &mut verdict)?;
    Ok(verdict)
}

/// Verifies the session log of the match played in `dir` with this machine's verifier, and
/// compares the server's final hash.
fn verify_run(dir: &Path) -> Result<Verdict, CheckError> {
    let binaries = build()?;
    let server = ProcessLog::read(Process::Server, &Process::Server.log_path(dir))?;
    let mut verdict = Verdict::default();
    verify(&binaries, dir, &server, &mut verdict)?;
    Ok(verdict)
}

/// The processes, built by the cargo that runs the check.
fn build() -> Result<Binaries, CheckError> {
    let cargo = env::var_os("CARGO").ok_or(CheckError::NotUnderCargo)?;
    Binaries::build(&cargo)
}

/// Checks that the server whose log is `server` wrote the session log into `dir`, and that this
/// machine's verifier ends without failure or warning, at the server's final hash.
fn verify(
    binaries: &Binaries,
    dir: &Path,
    server: &ProcessLog,
    verdict: &mut Verdict,
) -> Result<(), CheckError> {
    let written = server.first::<SessionWritten>()?;
    let verified = match &written {
        Some(written) => {
            let path = Process::Verifier.log_path(dir);
            let status = Command::new(&binaries.verifier)
                .arg(PACKAGES)
                .arg(&written.file)
                .current_dir(dir)
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
    Ok(())
}
