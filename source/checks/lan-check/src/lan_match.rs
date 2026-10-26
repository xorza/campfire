use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use campfire_common::{Bytes32, PlayerSlot, Tick};
use campfire_host::{ClientFrame, InputLogged, Listening, MatchStarted, ServerFrame};
use campfire_store::OutputFile;

use crate::binaries::Binaries;
use crate::error::CheckError;
use crate::log_tail::LogTail;
use crate::process::Process;
use crate::process_group::{POLL, ProcessGroup};
use crate::process_log::ProcessLog;
use crate::process_outcome::ProcessOutcome;

/// How long the server gets to listen, and then the whole match to end, before the check stops
/// what still runs, on a machine of `RunnerSpeed::Local`: a match of 11 s, with a restart its
/// clients notice in 5 s, ends in a few more.
const DEADLINE: Duration = Duration::from_secs(30);
/// How much longer CI's runners take: nextest's `ci` profile ends a test at 30 s where the default
/// ends it at 5 s.
const CI_SCALE: u32 = 6;
/// The server's data directory, in the run's directory.
pub(crate) const SERVER_DATA: &str = "server-data";
/// The local client's data directory, in the run's directory; its local server's is `server`
/// under it.
pub(crate) const LOCAL_DATA: &str = "local-data";
/// The bot the check stops once the server logged its first order and each other bot's order of
/// that stamp, and starts again with its key file.
pub(crate) const RESTARTED: usize = 1;
/// The check stops the server once it logged an input of this stamp or a later one, which the
/// second orders of the scripts are, and starts it again on its data directory.
const SERVER_STOP: Tick = Tick::new(50);

/// A match of the real server and one bot per script, each a process on `127.0.0.1`, each
/// logging JSON to a file in the run's directory; beside them, an impostor bot that pins the
/// wrong TLS key plays the first script.
#[derive(Debug)]
pub(crate) struct LanMatch<'a> {
    binaries: &'a Binaries,
    /// Where the logs, scripts and the session log go.
    dir: &'a Path,
    /// The mode package directory the server opens and the bots play.
    mode: &'a Path,
    /// The bots' order files.
    scripts: &'a [PathBuf],
    deadline: Duration,
    /// Each process's log, as the check read it while the process ran, so no line reads twice.
    logs: BTreeMap<Process, LogTail>,
}

/// How fast the machine that runs the check is, which sets its deadline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunnerSpeed {
    Local,
    /// One of CI's runners, `CI_SCALE` times slower.
    Ci,
}

/// How each process of a match ended: the server's first process and the one the check started
/// again after it, the bot `RESTARTED`'s first process among the bots, and the one the check
/// started again after it.
#[derive(Debug)]
pub(crate) struct Played {
    pub(crate) server: ProcessOutcome,
    pub(crate) server_again: ProcessOutcome,
    pub(crate) bots: Vec<ProcessOutcome>,
    pub(crate) rejoined: ProcessOutcome,
    pub(crate) impostor: ProcessOutcome,
}

/// What the first server's start came to: it listened, or it ended as the outcome says.
#[derive(Debug)]
enum Listened {
    Yes(Listening),
    No(ProcessOutcome),
}

impl Played {
    /// A match whose server never listened, which ended as `server` says, of `bots` bots none of
    /// which started.
    fn unplayed(server: ProcessOutcome, bots: usize) -> Self {
        Self {
            server,
            server_again: ProcessOutcome::NotStarted,
            bots: vec![ProcessOutcome::NotStarted; bots],
            rejoined: ProcessOutcome::NotStarted,
            impostor: ProcessOutcome::NotStarted,
        }
    }
}

impl RunnerSpeed {
    /// The speed that `profile`, the value of `NEXTEST_PROFILE`, names: CI's for nextest's `ci`
    /// profile, which CI's gate runs the tests with, as the check is a step of that gate.
    pub(crate) fn of(profile: Option<&OsStr>) -> Self {
        if profile == Some(OsStr::new("ci")) {
            Self::Ci
        } else {
            Self::Local
        }
    }

    const fn deadline(self) -> Duration {
        match self {
            Self::Local => DEADLINE,
            Self::Ci => DEADLINE.saturating_mul(CI_SCALE),
        }
    }
}

impl<'a> LanMatch<'a> {
    /// The match of `binaries` in `dir`, of the bots of `scripts` playing the mode in `mode`, at
    /// the deadline of `speed`.
    pub(crate) const fn new(
        binaries: &'a Binaries,
        dir: &'a Path,
        mode: &'a Path,
        scripts: &'a [PathBuf],
        speed: RunnerSpeed,
    ) -> Self {
        Self {
            binaries,
            dir,
            mode,
            scripts,
            deadline: speed.deadline(),
            logs: BTreeMap::new(),
        }
    }

    /// The log of `process` as far as it wrote it: the whole of one that ended.
    pub(crate) fn log(&mut self, process: Process) -> Result<&ProcessLog, CheckError> {
        self.tail(process).read()
    }

    fn tail(&mut self, process: Process) -> &mut LogTail {
        self.logs
            .entry(process)
            .or_insert_with(|| LogTail::new(process, self.dir))
    }

    /// Starts the server on a port the OS picks, waits until it listens; starts the impostor and
    /// waits until it ended, so its start shares the machine with no order; starts the bots;
    /// once the server logged the first order of bot `RESTARTED` and each other bot's order of
    /// that stamp, stops that bot and starts it again with its key file; once it logged an input
    /// stamped `SERVER_STOP` or later, stops the server and starts it again on its data directory
    /// and its port; and waits until every process ended or the deadline passed.
    pub(crate) fn play(&mut self) -> Result<Played, CheckError> {
        let deadline = Instant::now() + self.deadline;
        let mut children = ProcessGroup::default();
        let any_port = SocketAddr::from(([127, 0, 0, 1], 0));
        children.start(Process::Server, self.server(Process::Server, any_port)?);
        let Listening {
            address,
            tls_key,
            server_key,
            tick_hz,
            ..
        } = match self.listen(&mut children, deadline)? {
            Listened::Yes(listening) => listening,
            Listened::No(outcome) => return Ok(Played::unplayed(outcome, self.scripts.len())),
        };
        let wrong = Bytes32::new(tls_key.as_bytes().map(|byte| !byte)).to_string();
        let tls_key = tls_key.to_string();
        // The bots' starts borrow what the match holds apart from its logs, which the waits read.
        let Self {
            binaries,
            dir,
            mode,
            scripts,
            ..
        } = *self;
        let bot = |process: Process, key: Process, script: &Path, tls_key: &str| {
            let mut command = Command::new(&binaries.client);
            command
                .arg("--bot")
                .arg(script)
                .arg("--key")
                .arg(dir.join(format!("{}.nsec", key.file_stem())))
                .arg(mode)
                .arg(address.to_string())
                .arg(tls_key)
                .arg(server_key.to_string())
                .arg(tick_hz.to_string());
            start(dir, process, &mut command)
        };
        let impostor = Process::Impostor;
        children.start(impostor, bot(impostor, impostor, &scripts[0], &wrong)?);
        while children.try_wait(impostor)?.is_none() && Instant::now() < deadline {
            thread::sleep(POLL);
        }
        for (index, script) in scripts.iter().enumerate() {
            let process = Process::Bot(index);
            children.start(process, bot(process, process, script, &tls_key)?);
        }
        if self.first_orders_logged(deadline)? {
            let restarted = Process::Bot(RESTARTED);
            let again = Process::Rejoined(RESTARTED);
            let script = &scripts[RESTARTED];
            children.replace(restarted, again, || bot(again, restarted, script, &tls_key))?;
        }
        if self.logged_by(SERVER_STOP, deadline)? {
            children.replace(Process::Server, Process::ServerAgain, || {
                self.server(Process::ServerAgain, address)
            })?;
        }
        let mut ended = children.wait(deadline)?;
        let mut outcome = |process| ended.remove(&process).unwrap_or(ProcessOutcome::NotStarted);
        Ok(Played {
            server: outcome(Process::Server),
            server_again: outcome(Process::ServerAgain),
            bots: (0..self.scripts.len())
                .map(|bot| outcome(Process::Bot(bot)))
                .collect(),
            rejoined: outcome(Process::Rejoined(RESTARTED)),
            impostor: outcome(Process::Impostor),
        })
    }

    /// Waits until the first server among `children` listens, ends, or the deadline passed, when
    /// the check stops it.
    fn listen(
        &mut self,
        children: &mut ProcessGroup,
        deadline: Instant,
    ) -> Result<Listened, CheckError> {
        let mut seen = 0;
        loop {
            let log = self.tail(Process::Server);
            if let Some(listening) = log.read_past::<Listening>(&mut seen)?.into_iter().next() {
                return Ok(Listened::Yes(listening));
            }
            if Instant::now() >= deadline {
                return Ok(Listened::No(children.stop(Process::Server)?));
            }
            if let Some(outcome) = children.try_wait(Process::Server)? {
                return Ok(Listened::No(outcome));
            }
            thread::sleep(POLL);
        }
    }

    /// Starts the server as `process`, on its data directory, listening on `address`.
    fn server(&self, process: Process, address: SocketAddr) -> Result<Child, CheckError> {
        start(
            self.dir,
            process,
            Command::new(&self.binaries.server)
                .arg("--data")
                .arg(self.dir.join(SERVER_DATA))
                .arg(self.mode)
                .arg(address.to_string()),
        )
    }

    /// Waits until the server logged an input stamped `stamp` or later, or the deadline passed;
    /// whether it did.
    fn logged_by(&mut self, stamp: Tick, deadline: Instant) -> Result<bool, CheckError> {
        let mut seen = 0;
        while Instant::now() < deadline {
            let logged = self
                .tail(Process::Server)
                .read_past::<InputLogged>(&mut seen)?;
            if logged.iter().any(|input| input.stamp >= stamp) {
                return Ok(true);
            }
            thread::sleep(POLL);
        }
        Ok(false)
    }

    /// Waits until the server logged the first input of the player slot that bot `RESTARTED`
    /// learned it plays, and an input of each bot's player slot stamped as late or later, or the
    /// deadline passed; whether it did. A bot's order of that stamp still on its way would share
    /// the machine with the restarted bot's new process as it starts, and could miss its stamp
    /// tick.
    fn first_orders_logged(&mut self, deadline: Instant) -> Result<bool, CheckError> {
        let mut player_slots: Vec<Option<PlayerSlot>> = vec![None; self.scripts.len()];
        let mut seen_bots = vec![0; player_slots.len()];
        let mut seen_server = 0;
        let mut logged = Vec::new();
        while Instant::now() < deadline {
            for (bot, (player_slot, seen)) in
                player_slots.iter_mut().zip(&mut seen_bots).enumerate()
            {
                if player_slot.is_none() {
                    *player_slot = self
                        .tail(Process::Bot(bot))
                        .read_past::<MatchStarted>(seen)?
                        .first()
                        .map(|started| started.player_slot);
                }
            }
            logged.extend(
                self.tail(Process::Server)
                    .read_past::<InputLogged>(&mut seen_server)?,
            );
            if let Some(player_slots) = player_slots.iter().copied().collect::<Option<Vec<_>>>() {
                let first = logged
                    .iter()
                    .filter(|input| input.player_slot == player_slots[RESTARTED])
                    .map(|input| input.stamp)
                    .min();
                let reached = |stamp: Tick| {
                    player_slots.iter().all(|&player_slot| {
                        logged
                            .iter()
                            .any(|input| input.player_slot == player_slot && input.stamp >= stamp)
                    })
                };
                if first.is_some_and(reached) {
                    return Ok(true);
                }
            }
            thread::sleep(POLL);
        }
        Ok(false)
    }

    /// Plays a client bot of the first script alone on a local server with no network, a server
    /// bot of the second in player slot 1, and waits until it ended or the deadline passed.
    pub(crate) fn play_local(&self) -> Result<ProcessOutcome, CheckError> {
        let deadline = Instant::now() + self.deadline;
        let mut children = ProcessGroup::default();
        let client = start(
            self.dir,
            Process::Local,
            Command::new(&self.binaries.client)
                .arg("--local")
                .arg("--data")
                .arg(self.dir.join(LOCAL_DATA))
                .arg("--server-orders")
                .arg(format!("1={}", self.scripts[1].display()))
                .arg("--bot")
                .arg(&self.scripts[0])
                .arg("--key")
                .arg(
                    self.dir
                        .join(format!("{}.nsec", Process::Local.file_stem())),
                )
                .arg(self.mode),
        )?;
        children.start(Process::Local, client);
        let mut ended = children.wait(deadline)?;
        Ok(ended.remove(&Process::Local).expect("the local client ran"))
    }
}

/// What each process's JSON file holds beyond its own file filter: the start of every frame of
/// the server and of each client, which their filters leave out, so a failed order shows when
/// each end ran each frame between its send, its receipt and its tick. It names the targets the
/// events log at, so a move of an event moves its directive.
fn log_extra() -> String {
    format!(
        "{}=trace,{}=trace",
        ServerFrame::TARGET,
        ClientFrame::TARGET
    )
}

/// Starts `command` as `process`, in the run's directory `dir`, logging JSON to its file and text
/// to a file beside it. Its file filter is the binary's own, whatever the check's environment says,
/// as the verdict reads its events at Debug.
fn start(dir: &Path, process: Process, command: &mut Command) -> Result<Child, CheckError> {
    let text = dir.join(format!("{}.log", process.file_stem()));
    let text = OutputFile::stdio(&text).map_err(CheckError::TextLog)?;
    command
        .current_dir(dir)
        .env("CAMPFIRE_LOG", process.log_path(dir))
        .env_remove("CAMPFIRE_LOG_FILTER")
        .env("CAMPFIRE_LOG_FILTER_EXTRA", log_extra())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(text)
        .spawn()
        .map_err(|error| CheckError::Start { process, error })
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::capture_named;
    use campfire_log::{LogEvent, LogLine};
    use tracing::trace;

    use super::*;

    #[test]
    fn the_extra_filter_keeps_each_frames_start_and_reads_it_back() {
        // A server frame of tick 20 and a client frame of tick 21 log under the filter and read
        // back; an event at Trace of another target does not log.
        let (server, client) = (
            ServerFrame {
                tick: Tick::new(20),
            },
            ClientFrame {
                tick: Tick::new(21),
            },
        );
        let lines = capture_named(&log_extra(), || {
            server.log();
            trace!("a frame of no check");
            client.log();
        });
        assert_eq!(lines.len(), 2, "{lines:?}");
        let read = |line: &str| LogLine::parse(line).unwrap();
        let server_read = read(&lines[0]).read::<ServerFrame>().unwrap().ok();
        let client_read = read(&lines[1]).read::<ClientFrame>().unwrap().ok();
        assert_eq!((server_read, client_read), (Some(server), Some(client)));
    }

    #[test]
    fn cis_profile_scales_the_deadline() {
        // 30 s on a machine of its own, 6 × 30 s = 180 s on CI's runners.
        for (profile, speed, deadline) in [
            (None, RunnerSpeed::Local, 30),
            (Some("default"), RunnerSpeed::Local, 30),
            (Some("ci"), RunnerSpeed::Ci, 180),
        ] {
            let read = RunnerSpeed::of(profile.map(OsStr::new));
            assert_eq!(read, speed);
            assert_eq!(read.deadline(), Duration::from_secs(deadline));
        }
    }
}
