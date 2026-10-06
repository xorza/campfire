//! LAN check on request: builds and starts the real `campfire-server` and two
//! `campfire-client --bot` processes over WebTransport on `127.0.0.1`, for a match of about 3 s
//! of the test lane mode, and a third bot that pins the wrong certificate; once the server logged
//! the second bot's first order, it stops that bot and starts it again with its key file. Then it
//! plays a client bot of the first script against a server bot of the second on a local server,
//! `campfire-client --local`, with no network. It reads their JSON logs and runs
//! `campfire-verifier` on each session log. It passes when every process succeeded, the bot it
//! stopped ran until it did, and none logged a warning or an error; every order a bot process
//! sent was logged and took effect in its stamp tick, or right after a frame of several ticks it
//! waited for, the second bot started again sending each order of its script, late or not; each
//! verifier gives its host's final hash; and the third bot exited with failure and logged why.
//!
//! Run it with `cargo run -p campfire-lan-check [-- <run root>]`. Each run's logs and session log
//! go into a new directory below the run root, named for the run's start. `cargo run -p
//! campfire-lan-check -- verify <run directory>` verifies the session log of a run, perhaps from
//! another machine, with this machine's verifier, and compares the server's final hash.

use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::time::SystemTime;

use campfire_log::Logging;
use campfire_net::{
    InputLogged, LinkLost, Listening, OrderScript, SessionDir, SessionWritten, TicksCaughtUp,
};
use campfire_package::PackageDir;
use campfire_verifier::Verified;
use tracing::{error, info};

use crate::binaries::Binaries;
use crate::error::CheckError;
use crate::lan_match::{LOCAL_DATA, LanMatch, RESTARTED, SERVER_DATA};
use crate::mode::Mode;
use crate::outcome::Outcome;
use crate::process::Process;
use crate::process_log::ProcessLog;
use crate::run_dir::RunDir;
use crate::session_kind::SessionKind;
use crate::verdict::{BotEvents, Verdict};

mod binaries;
mod error;
mod failure;
mod lan_match;
mod mode;
mod outcome;
mod process;
mod process_log;
mod run_dir;
mod session_kind;
mod target_name;
mod verdict;

/// The test lane mode, a hero a side, 30 ticks a second, within the workspace's packages.
const MODE: &str = "test/modes/lane";
/// The packages the verifier holds, the lane mode's among them.
const PACKAGES: &str = "test";
/// Each bot's orders: two steps near its spawn and a cast of its first ability, then it leaves
/// after tick 90, 3 s into the match. The walker's cast hits whoever stands within 2 m; the
/// runner's names no target, which its ability needs, so the sim refuses it, but the order is an
/// input all the same.
const SCRIPTS: [&str; 2] = [
    "end = 90\n[[order]]\ntick = 20\nmove = [2, 0]\n[[order]]\ntick = 50\nmove = [-1, 2]\n\
     [[order]]\ntick = 70\ncast = 0\n",
    "end = 90\n[[order]]\ntick = 20\nmove = [-2, 0]\n[[order]]\ntick = 50\nmove = [1, -2]\n\
     [[order]]\ntick = 70\ncast = 0\n",
];

fn main() -> ExitCode {
    Logging {
        terminal: "info",
        file: "info",
    }
    .start();
    let Some(mode) = Mode::parse(env::args_os().skip(1)) else {
        error!("usage: campfire-lan-check [<run root>] | verify <run directory>");
        return ExitCode::from(2);
    };
    let dir = match &mode {
        Mode::Play { root } => match RunDir::create(root, SystemTime::now()) {
            Ok(run) => run.path().to_owned(),
            Err(error) => {
                error!(%error, "the LAN check did not run");
                return ExitCode::FAILURE;
            }
        },
        Mode::Verify { dir } => dir.clone(),
    };
    let result = match mode {
        Mode::Play { .. } => play(&dir),
        Mode::Verify { .. } => verify_run(&dir),
    };
    report(result, &dir)
}

/// Logs each failure of `result`, then a last line with their number and the run's `dir`.
fn report(result: Result<Verdict, CheckError>, dir: &Path) -> ExitCode {
    let verdict = match result {
        Ok(verdict) => verdict,
        Err(error) => {
            error!(%error, dir = %dir.display(), "the LAN check did not run");
            return ExitCode::FAILURE;
        }
    };
    let mut failures = 0;
    for failure in verdict.failures() {
        error!(%failure, "a failure of the LAN check");
        failures += 1;
    }
    if failures == 0 {
        info!(dir = %dir.display(), "the LAN check passed");
        ExitCode::SUCCESS
    } else {
        error!(failures, dir = %dir.display(), "the LAN check failed");
        ExitCode::FAILURE
    }
}

/// Plays a LAN match with its logs in `dir`, a new run's directory, and checks it.
fn play(dir: &Path) -> Result<Verdict, CheckError> {
    let binaries = build()?;
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
        mode: &PackageDir::workspace(MODE),
        scripts: &scripts,
    };
    let played = lan.play()?;
    let mut verdict = Verdict::default();
    let server = ProcessLog::read(Process::Server, &Process::Server.log_path(dir))?;
    verdict.process(Process::Server, played.server, &server);
    verdict.listened(&server.read_all::<Listening>()?);
    let mut bots = Vec::with_capacity(played.bots.len() + 1);
    for ((bot, &outcome), &scripted) in played.bots.iter().enumerate().zip(&scripted) {
        let process = Process::Bot(bot);
        let log = ProcessLog::read(process, &process.log_path(dir))?;
        if bot == RESTARTED {
            verdict.stopped(process, outcome, &log);
        } else {
            verdict.process(process, outcome, &log);
        }
        bots.push(BotEvents::of(bot, &log, scripted, bot != RESTARTED)?);
    }
    let again = Process::Rejoined(RESTARTED);
    let log = ProcessLog::read(again, &again.log_path(dir))?;
    verdict.process(again, played.rejoined, &log);
    bots.push(BotEvents::of(RESTARTED, &log, scripted[RESTARTED], true)?);
    let caught_up = server.read_all::<TicksCaughtUp>()?;
    verdict.orders(&server.read_all::<InputLogged>()?, &caught_up, &bots);
    let impostor = ProcessLog::read(Process::Impostor, &Process::Impostor.log_path(dir))?;
    verdict.impostor(played.impostor, &impostor.read_all::<LinkLost>()?);
    verify(&binaries, dir, SessionKind::Lan, &mut verdict)?;

    let local = lan.play_local()?;
    let log = ProcessLog::read(Process::Local, &Process::Local.log_path(dir))?;
    verdict.process(Process::Local, local, &log);
    verify(&binaries, dir, SessionKind::Local, &mut verdict)?;
    Ok(verdict)
}

/// Verifies the session logs of the matches played in `dir` with this machine's verifier, and
/// compares each host's final hash.
fn verify_run(dir: &Path) -> Result<Verdict, CheckError> {
    let binaries = build()?;
    let mut verdict = Verdict::default();
    for session in [SessionKind::Lan, SessionKind::Local] {
        verify(&binaries, dir, session, &mut verdict)?;
    }
    Ok(verdict)
}

/// The processes, built by the cargo that runs the check.
fn build() -> Result<Binaries, CheckError> {
    let cargo = env::var_os("CARGO").ok_or(CheckError::NotUnderCargo)?;
    Binaries::build(&cargo)
}

/// Checks that the host of `session` wrote the session log into `dir`, and that this machine's
/// verifier ends without failure or warning, at the host's final hash.
fn verify(
    binaries: &Binaries,
    dir: &Path,
    session: SessionKind,
    verdict: &mut Verdict,
) -> Result<(), CheckError> {
    let host = session.host();
    let written = ProcessLog::read(host, &host.log_path(dir))?.first::<SessionWritten>()?;
    let data = match session {
        SessionKind::Lan => dir.join(SERVER_DATA),
        SessionKind::Local => dir.join(LOCAL_DATA).join("server"),
    };
    let replayer = session.verifier();
    let verified = match &written {
        Some(written) => {
            // The log in the run's directory: `written.file` is a path on the machine that played
            // the run, in that system's syntax, which another may not read.
            let file = SessionDir::published(&data, written.session);
            let path = replayer.log_path(dir);
            let status = Command::new(&binaries.verifier)
                .arg(PackageDir::workspace(PACKAGES))
                .arg(file)
                .current_dir(dir)
                .env("CAMPFIRE_LOG", &path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|error| CheckError::Start {
                    process: replayer,
                    error,
                })?;
            let log = ProcessLog::read(replayer, &path)?;
            verdict.process(replayer, Outcome::of(status), &log);
            log.first::<Verified>()?
        }
        None => None,
    };
    verdict.hash(session, written.as_ref(), verified.as_ref());
    Ok(())
}
