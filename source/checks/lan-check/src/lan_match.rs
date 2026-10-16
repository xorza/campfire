use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use campfire_common::{Bytes32, PlayerSlot, Tick};
use campfire_net::{InputLogged, Listening, MatchStarted};
use campfire_store::OutputFile;

use crate::binaries::Binaries;
use crate::error::CheckError;
use crate::log_tail::LogTail;
use crate::process::Process;
use crate::process_group::{POLL, ProcessGroup};
use crate::process_outcome::ProcessOutcome;

/// How long the server gets to listen, and then the whole match to end, before the check stops
/// what still runs: a match of 11 s, with a restart its clients notice in 5 s, ends in a few more.
const DEADLINE: Duration = Duration::from_secs(30);
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
/// What each process's JSON file holds beyond its own file filter: the start of every frame of
/// the server and of each client, which their filters leave out, so a failed order shows when
/// each end ran each frame between its send, its receipt and its tick.
const LOG_EXTRA: &str =
    "campfire_net::events::server_frame=trace,campfire_net::events::client_frame=trace";

/// A match of the real server and one bot per script, each a process on `127.0.0.1`, each
/// logging JSON to a file in the run's directory; beside them, an impostor bot that pins the
/// wrong TLS key plays the first script.
#[derive(Debug)]
pub(crate) struct LanMatch<'a> {
    pub(crate) binaries: &'a Binaries,
    /// Where the logs, scripts and the session log go.
    pub(crate) dir: &'a Path,
    /// The mode package directory the server opens and the bots play.
    pub(crate) mode: &'a Path,
    /// The bots' order files.
    pub(crate) scripts: &'a [PathBuf],
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

impl LanMatch<'_> {
    /// Starts the server on a port the OS picks, waits until it listens, starts the bots and the
    /// impostor; once the server logged the first order of bot `RESTARTED` and each other bot's
    /// order of that stamp, stops that bot and starts it again with its key file; once it logged
    /// an input stamped `SERVER_STOP` or later, stops the server and starts it again on its data
    /// directory and its port; and waits until every process ended or the deadline passed.
    pub(crate) fn play(&self) -> Result<Played, CheckError> {
        let deadline = Instant::now() + DEADLINE;
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
        let bots = self
            .scripts
            .iter()
            .enumerate()
            .map(|(index, script)| (Process::Bot(index), script, tls_key.to_string()));
        let impostor = (Process::Impostor, &self.scripts[0], wrong);
        let bot = |process: Process, key: Process, script: &Path, tls_key: &str| {
            let mut command = Command::new(&self.binaries.client);
            command
                .arg("--bot")
                .arg(script)
                .arg("--key")
                .arg(self.dir.join(format!("{}.nsec", key.file_stem())))
                .arg(self.mode)
                .arg(address.to_string())
                .arg(tls_key)
                .arg(server_key.to_string())
                .arg(tick_hz.to_string());
            self.start(process, &mut command)
        };
        for (process, script, tls_key) in bots.chain([impostor]) {
            children.start(process, bot(process, process, script, &tls_key)?);
        }
        if self.first_orders_logged(deadline)? {
            let restarted = Process::Bot(RESTARTED);
            let again = Process::Rejoined(RESTARTED);
            let script = &self.scripts[RESTARTED];
            children.replace(restarted, again, || {
                bot(again, restarted, script, &tls_key.to_string())
            })?;
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
        &self,
        children: &mut ProcessGroup,
        deadline: Instant,
    ) -> Result<Listened, CheckError> {
        let mut log = LogTail::new(Process::Server, self.dir);
        loop {
            if let Some(listening) = log.read::<Listening>()?.into_iter().next() {
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
        self.start(
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
    fn logged_by(&self, stamp: Tick, deadline: Instant) -> Result<bool, CheckError> {
        let mut log = LogTail::new(Process::Server, self.dir);
        while Instant::now() < deadline {
            let logged = log.read::<InputLogged>()?;
            if logged.iter().any(|input| input.stamp >= stamp) {
                return Ok(true);
            }
            thread::sleep(POLL);
        }
        Ok(false)
    }

    /// Waits until the server logged the first input of the slot that bot `RESTARTED` learned it
    /// plays, and an input of each bot's slot stamped as late or later, or the deadline passed;
    /// whether it did. A bot's order of that stamp still on its way would share the machine with
    /// the restarted bot's new process as it starts, and could miss its stamp tick.
    fn first_orders_logged(&self, deadline: Instant) -> Result<bool, CheckError> {
        let mut bots: Vec<LogTail> = (0..self.scripts.len())
            .map(|bot| LogTail::new(Process::Bot(bot), self.dir))
            .collect();
        let mut slots: Vec<Option<PlayerSlot>> = vec![None; bots.len()];
        let mut server = LogTail::new(Process::Server, self.dir);
        let mut logged = Vec::new();
        while Instant::now() < deadline {
            for (log, slot) in bots.iter_mut().zip(&mut slots) {
                if slot.is_none() {
                    *slot = log
                        .read::<MatchStarted>()?
                        .first()
                        .map(|started| started.slot);
                }
            }
            logged.extend(server.read::<InputLogged>()?);
            if let Some(slots) = slots.iter().copied().collect::<Option<Vec<_>>>() {
                let first = logged
                    .iter()
                    .filter(|input| input.slot == slots[RESTARTED])
                    .map(|input| input.stamp)
                    .min();
                let reached = |stamp: Tick| {
                    slots.iter().all(|&slot| {
                        logged
                            .iter()
                            .any(|input| input.slot == slot && input.stamp >= stamp)
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
    /// bot of the second in slot 1, and waits until it ended or the deadline passed.
    pub(crate) fn play_local(&self) -> Result<ProcessOutcome, CheckError> {
        let deadline = Instant::now() + DEADLINE;
        let mut children = ProcessGroup::default();
        let client = self.start(
            Process::Local,
            Command::new(&self.binaries.client)
                .arg("--local")
                .arg("--data")
                .arg(self.dir.join(LOCAL_DATA))
                .arg("--server-bot")
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

    /// Starts `command` as `process`, in the run's directory, logging JSON to its file and text to
    /// a file beside it. Its file filter is the binary's own, whatever the check's environment
    /// says, as the verdict reads its events at Debug.
    fn start(&self, process: Process, command: &mut Command) -> Result<Child, CheckError> {
        let text = self.dir.join(format!("{}.log", process.file_stem()));
        let text = OutputFile::stdio(&text).map_err(CheckError::TextLog)?;
        command
            .current_dir(self.dir)
            .env("CAMPFIRE_LOG", process.log_path(self.dir))
            .env_remove("CAMPFIRE_LOG_FILTER")
            .env("CAMPFIRE_LOG_FILTER_EXTRA", LOG_EXTRA)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(text)
            .spawn()
            .map_err(|error| CheckError::Start { process, error })
    }
}
