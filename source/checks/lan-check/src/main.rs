//! LAN check on request: plays the real server and client bots over UDP on `127.0.0.1`, with a
//! bot stopped and started again, a server restarted on its data, and a bot that pins the wrong
//! key, then a local match; it passes when each process, log and verified session log is as the
//! match played.
//!
//! Run it with `cargo drive run lan`, which builds it and the processes it starts in one build,
//! beside it, and gives it the run root `target/drive/lan-check`; each run's logs and session log
//! go into a new directory below the run root, named for the run's start, which keeps the newest
//! `KEPT_RUNS`. `campfire-lan-check verify <run directory>` verifies the session log of a run,
//! perhaps from another machine, with the verifier beside the check, and compares the server's
//! final hash; with `--verifier <verifier>`, it runs that verifier.

use std::env;
use std::num::NonZeroUsize;
use std::path::{self, Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::SystemTime;

use campfire_bots::OrderScript;
use campfire_common::ErrorReport;
use campfire_common::ExitStatus;
use campfire_host::{ClientLayout, Listening, ServerLayout, SessionWritten, TicksCaughtUp};
use campfire_logging::Logging;
use campfire_net::LinkLost;
use campfire_protocol::{AppliedInput, SessionLog};
use campfire_store::{DurableFile, InputFile, RunDir};
use campfire_verifier::Verified;
use tracing::{error, info};

use crate::binaries::Binaries;
use crate::command_line::Args;
use crate::error::CheckError;
use crate::lan_match::{LOCAL_DATA, LanMatch, RESTARTED, RunnerSpeed, SERVER_DATA};
use crate::process::Process;
use crate::process_log::ProcessLog;
use crate::process_outcome::ProcessOutcome;
use crate::session_kind::SessionKind;
use crate::verdict::{BotEvents, Verdict};

mod binaries;
mod command_line;
mod error;
mod failure;
mod lan_match;
mod log_tail;
mod process;
mod process_group;
mod process_log;
mod process_outcome;
mod session_kind;
mod verdict;

/// The runs the run root keeps, this one among them: each holds 6 to 14 MB of logs, and the newest
/// 3 hold a failure and the runs around it, as pytest keeps its newest 3 `tmp_path` directories.
const KEPT_RUNS: NonZeroUsize = NonZeroUsize::new(3).unwrap();
/// The workspace's `packages` directory, where the check finds the test packages. It is not
/// `campfire-package-disk`'s `PackageDir::workspace`, which only its `internals` feature has, and
/// the build that ships, which `ship` lints, has no `internals`.
const PACKAGES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages");
/// The test lane mode, a hero a side, 30 ticks a second, within the workspace's packages.
const MODE: &str = "test/engine/modes/lane";
/// The packages the verifier holds, the lane mode's among them.
const PACKAGES: &str = "test/engine";
/// Each bot's orders: two steps near its spawn and a cast of its first ability, a third step at
/// tick 300, after the server's restart, then it leaves after tick 330, 11 s into the match: a
/// client notices a server that stopped by its packet session's idle timeout, 5 s, and comes back
/// to it before its script ends. The walker's cast hits whoever stands within 2 m; the runner's
/// names no target, which its ability needs, so the sim refuses it, but the order is an input all
/// the same.
const SCRIPTS: [&str; 2] = [
    "end = 330\n[[order]]\ntick = 20\ndirective.orders.move.to = [2, 0]\n[[order]]\ntick = 50\ndirective.orders.move.to = [-1, 2]\n\
     [[order]]\ntick = 70\ndirective.orders.slot = { slot = \"first\", target = \"none\" }\n\
     [[order]]\ntick = 300\ndirective.orders.move.to = [0, 1]\n",
    "end = 330\n[[order]]\ntick = 20\ndirective.orders.move.to = [-2, 0]\n[[order]]\ntick = 50\ndirective.orders.move.to = [1, -2]\n\
     [[order]]\ntick = 70\ndirective.orders.slot = { slot = \"first\", target = \"none\" }\n\
     [[order]]\ntick = 300\ndirective.orders.move.to = [0, -1]\n",
];

fn main() -> ExitCode {
    let _log = Logging {
        terminal: "info",
        file: "info",
    }
    .start();
    let args = match Logging::command_line(env::args_os()) {
        Ok(line) => Args::of(line),
        Err(status) => return ExitCode::from(status),
    };
    let dir = match &args {
        Args::Play { root } => match RunDir::create(root, SystemTime::now(), KEPT_RUNS) {
            Ok(run) => run.path().to_owned(),
            Err(error) => {
                error!(error = %ErrorReport::of(&error), "the LAN check did not run");
                return ExitCode::from(ExitStatus::Failure);
            }
        },
        Args::Verify { dir, .. } => dir.clone(),
    };
    let result = match args {
        Args::Play { .. } => play(&dir),
        Args::Verify { verifier, .. } => verify_run(&dir, verifier),
    };
    report(result, &dir)
}

/// Logs each failure of `result`, then a last line with their number and the run's `dir`.
fn report(result: Result<Verdict, CheckError>, dir: &Path) -> ExitCode {
    let verdict = match result {
        Ok(verdict) => verdict,
        Err(error) => {
            error!(error = %ErrorReport::of(&error), dir = %dir.display(), "the LAN check did not run");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let mut failures = 0;
    for failure in verdict.failures() {
        error!(%failure, "a failure of the LAN check");
        failures += 1;
    }
    if failures == 0 {
        info!(dir = %dir.display(), "the LAN check passed");
        ExitCode::from(ExitStatus::Success)
    } else {
        error!(failures, dir = %dir.display(), "the LAN check failed");
        ExitCode::from(ExitStatus::Failure)
    }
}

/// Plays a LAN match with its logs in `dir`, a new run's directory, and checks it.
fn play(dir: &Path) -> Result<Verdict, CheckError> {
    let binaries = Binaries::beside_check()?;
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
        DurableFile::write(&path, text.as_bytes()).map_err(CheckError::Write)?;
        scripts.push(path);
    }

    let mode = Path::new(PACKAGES_DIR).join(MODE);
    let speed = RunnerSpeed::of(env::var_os("NEXTEST_PROFILE").as_deref());
    let mut lan = LanMatch::new(&binaries, dir, &mode, &scripts, speed);
    let played = lan.play()?;
    let mut verdict = Verdict::default();
    let server = lan.log(Process::Server)?;
    verdict.stopped(Process::Server, played.server, server);
    verdict.listened(&server.read_all::<Listening>()?);
    let mut caught_up = server.read_all::<TicksCaughtUp>()?;
    let again = Process::ServerAgain;
    let server_again = lan.log(again)?;
    verdict.process(again, played.server_again, server_again);
    caught_up.extend(server_again.read_all::<TicksCaughtUp>()?);
    let written = server_again.first::<SessionWritten>()?;
    let mut bots = Vec::with_capacity(played.bots.len() + 1);
    for ((bot, &outcome), &scripted) in played.bots.iter().enumerate().zip(&scripted) {
        let process = Process::Bot(bot);
        let log = lan.log(process)?;
        if bot == RESTARTED {
            verdict.stopped(process, outcome, log);
        } else {
            verdict.process(process, outcome, log);
        }
        bots.push(BotEvents::of(bot, log, scripted, bot != RESTARTED)?);
    }
    let again = Process::Rejoined(RESTARTED);
    let log = lan.log(again)?;
    verdict.process(again, played.rejoined, log);
    bots.push(BotEvents::of(RESTARTED, log, scripted[RESTARTED], true)?);
    verdict.orders(&published_inputs(dir, written.as_ref())?, &caught_up, &bots);
    let impostor = lan.log(Process::Impostor)?;
    verdict.impostor(played.impostor, &impostor.read_all::<LinkLost>()?);
    let verifier = RunVerifier {
        executable: &binaries.verifier,
        dir,
    };
    verifier.verify(SessionKind::Lan, written.as_ref(), &mut verdict)?;

    let local = lan.play_local()?;
    let log = lan.log(Process::Local)?;
    verdict.process(Process::Local, local, log);
    let written = log.first::<SessionWritten>()?;
    verifier.verify(SessionKind::Local, written.as_ref(), &mut verdict)?;
    Ok(verdict)
}

/// The inputs of the session log that the server published in `dir`, as `written` names it, each
/// with the tick it took effect in; none when it published none.
fn published_inputs(
    dir: &Path,
    written: Option<&SessionWritten>,
) -> Result<Vec<AppliedInput>, CheckError> {
    let Some(written) = written else {
        return Ok(Vec::new());
    };
    let path = host_layout(dir, SessionKind::Lan).published_log(written.session);
    let bytes = InputFile::read(&path, SessionLog::MAX_FILE_LEN).map_err(CheckError::Read)?;
    let published = SessionLog::decode(&bytes).map_err(CheckError::SessionLog)?;
    Ok(published.applied_inputs())
}

/// The layout of the data directory in `dir` of the host of `session`, which ended: the
/// server's, or the local server's under the client's. Read by its layout alone, with no lock
/// and no access check: a run another platform played comes as an artifact that keeps neither.
fn host_layout(dir: &Path, session: SessionKind) -> ServerLayout {
    match session {
        SessionKind::Lan => ServerLayout::at(dir.join(SERVER_DATA)),
        SessionKind::Local => {
            ServerLayout::at(ClientLayout::at(dir.join(LOCAL_DATA)).local_server_dir())
        }
    }
}

/// Verifies the session logs of the matches played in `dir` with `verifier`, or the one beside
/// the check when none is given, and compares each host's final hash.
fn verify_run(dir: &Path, verifier: Option<PathBuf>) -> Result<Verdict, CheckError> {
    let verifier = match verifier {
        // The verifier runs in the run's directory, where a relative path would name another file.
        Some(verifier) => path::absolute(verifier).map_err(CheckError::WorkingDir)?,
        None => Binaries::beside_check()?.verifier,
    };
    let mut verdict = Verdict::default();
    let verifier = RunVerifier {
        executable: &verifier,
        dir,
    };
    for session in [SessionKind::Lan, SessionKind::Local] {
        let host = session.host();
        let written = ProcessLog::read(host, &host.log_path(dir))?.first::<SessionWritten>()?;
        verifier.verify(session, written.as_ref(), &mut verdict)?;
    }
    Ok(verdict)
}

/// The verifier `executable`, which checks the logs of the run in `dir`.
#[derive(Debug, Clone, Copy)]
struct RunVerifier<'a> {
    executable: &'a Path,
    dir: &'a Path,
}

impl RunVerifier<'_> {
    /// Checks that the host of `session` wrote the session log into the run's directory, as
    /// `written`, its line of the host's log, names it, and that the verifier ends without
    /// failure or warning, at the host's final hash, into `verdict`.
    fn verify(
        self,
        session: SessionKind,
        written: Option<&SessionWritten>,
        verdict: &mut Verdict,
    ) -> Result<(), CheckError> {
        let RunVerifier { executable, dir } = self;
        let replayer = session.verifier();
        let verified = match written {
            Some(written) => {
                // The log in the run's directory: `written.file` is a path on the machine that
                // played the run, in that system's syntax, which another may not read.
                let file = host_layout(dir, session).published_log(written.session);
                let path = replayer.log_path(dir);
                let status = Command::new(executable)
                    .arg(Path::new(PACKAGES_DIR).join(PACKAGES))
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
                verdict.process(replayer, ProcessOutcome::of(status), &log);
                log.first::<Verified>()?
            }
            None => None,
        };
        verdict.hash(session, written, verified.as_ref());
        Ok(())
    }
}
