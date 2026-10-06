use std::fs::File;
use std::mem;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use campfire_common::{Bytes32, Tick};
use campfire_net::{InputLogged, Listening, MatchStarted};

use crate::binaries::Binaries;
use crate::error::CheckError;
use crate::outcome::Outcome;
use crate::process::Process;
use crate::process_log::ProcessLog;

/// How long the server gets to listen, and then the whole match to end, before the check stops
/// what still runs: a match of 11 s, with a restart its clients notice in 5 s, ends in a few more.
const DEADLINE: Duration = Duration::from_secs(30);
/// How often the check looks at the processes while it waits.
const POLL: Duration = Duration::from_millis(50);
/// The server's data directory, in the run's directory.
pub(crate) const SERVER_DATA: &str = "server-data";
/// The local client's data directory, in the run's directory; its local server's is `server`
/// under it.
pub(crate) const LOCAL_DATA: &str = "local-data";
/// The bot the check stops once the server logged its first order, and starts again with its key
/// file.
pub(crate) const RESTARTED: usize = 1;
/// The check stops the server once it logged an input of this stamp or a later one, which the
/// second orders of the scripts are, and starts it again on its data directory.
const SERVER_STOP: Tick = Tick::new(50);

/// A match of the real server and one bot per script, each a process on `127.0.0.1`, each
/// logging JSON to a file in the run's directory; beside them, an impostor bot that pins the
/// wrong certificate plays the first script.
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
    pub(crate) server: Outcome,
    pub(crate) server_again: Outcome,
    pub(crate) bots: Vec<Outcome>,
    pub(crate) rejoined: Outcome,
    pub(crate) impostor: Outcome,
}

/// What the first server's start came to: it listened, or it ended as the outcome says.
#[derive(Debug)]
enum Listened {
    Yes(Listening),
    No(Outcome),
}

impl Played {
    /// A match whose server never listened, which ended as `server` says, of `bots` bots none of
    /// which started.
    fn unplayed(server: Outcome, bots: usize) -> Played {
        Played {
            server,
            server_again: Outcome::NotStarted,
            bots: vec![Outcome::NotStarted; bots],
            rejoined: Outcome::NotStarted,
            impostor: Outcome::NotStarted,
        }
    }
}

impl LanMatch<'_> {
    /// Starts the server, waits until it listens, starts the bots and the impostor; once the
    /// server logged the first order of bot `RESTARTED`, stops that bot and starts it again with
    /// its key file; once it logged an input stamped `SERVER_STOP` or later, stops the server
    /// and starts it again on its data directory; and waits until every process ended or the
    /// deadline passed.
    pub(crate) fn play(&self) -> Result<Played, CheckError> {
        let deadline = Instant::now() + DEADLINE;
        let address = SocketAddr::from(([127, 0, 0, 1], free_port()?));
        let mut server = self.server(Process::Server, address)?;
        let Listening {
            certificate,
            server_key,
            tick_hz,
            ..
        } = match self.listen(&mut server, deadline)? {
            Listened::Yes(listening) => listening,
            Listened::No(outcome) => return Ok(Played::unplayed(outcome, self.scripts.len())),
        };
        let mut children = vec![(Process::Server, server)];
        let wrong = Bytes32::new(certificate.as_bytes().map(|byte| !byte)).to_string();
        let bots = self
            .scripts
            .iter()
            .enumerate()
            .map(|(index, script)| (Process::Bot(index), script, certificate.to_string()));
        let impostor = (Process::Impostor, &self.scripts[0], wrong);
        let bot = |process: Process, key: Process, script: &Path, certificate: &str| {
            let mut command = Command::new(&self.binaries.client);
            command
                .arg("--bot")
                .arg(script)
                .arg("--key")
                .arg(self.dir.join(format!("{}.nsec", key.file_stem())))
                .arg(self.mode)
                .arg(address.to_string())
                .arg(certificate)
                .arg(server_key.to_string())
                .arg(tick_hz.to_string());
            self.start(process, &mut command)
        };
        for (process, script, certificate) in bots.chain([impostor]) {
            children.push((process, bot(process, process, script, &certificate)?));
        }
        let restarted = Process::Bot(RESTARTED);
        let stopped = if self.first_order_logged(restarted, deadline)? {
            let at = 1 + RESTARTED;
            kill(restarted, &mut children[at].1)?;
            let again = Process::Rejoined(RESTARTED);
            let script = &self.scripts[RESTARTED];
            children[at] = (
                again,
                bot(again, restarted, script, &certificate.to_string())?,
            );
            Some(Outcome::Stopped)
        } else {
            None
        };
        let restored = if self.logged_by(SERVER_STOP, deadline)? {
            kill(Process::Server, &mut children[0].1)?;
            children[0] = (
                Process::ServerAgain,
                self.server(Process::ServerAgain, address)?,
            );
            true
        } else {
            false
        };
        let mut ended = wait(&mut children, deadline)?;
        let (server, server_again) = if restored {
            (Outcome::Stopped, ended.remove(0))
        } else {
            (ended.remove(0), Outcome::NotStarted)
        };
        let impostor = ended.pop().expect("the impostor ran");
        let rejoined = match stopped {
            Some(outcome) => mem::replace(&mut ended[RESTARTED], outcome),
            None => Outcome::NotStarted,
        };
        Ok(Played {
            server,
            server_again,
            bots: ended,
            rejoined,
            impostor,
        })
    }

    /// Waits until the first server, `server`, listens, ends, or the deadline passed, when the
    /// check stops it.
    fn listen(&self, server: &mut Child, deadline: Instant) -> Result<Listened, CheckError> {
        let log = Process::Server.log_path(self.dir);
        loop {
            if let Some(listening) =
                ProcessLog::read(Process::Server, &log)?.first::<Listening>()?
            {
                return Ok(Listened::Yes(listening));
            }
            if Instant::now() >= deadline {
                return Ok(Listened::No(stop(Process::Server, server)?));
            }
            if let Some(status) = server.try_wait().map_err(|error| CheckError::Wait {
                process: Process::Server,
                error,
            })? {
                return Ok(Listened::No(Outcome::of(status)));
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
        let path = Process::Server.log_path(self.dir);
        while Instant::now() < deadline {
            let logged = ProcessLog::read(Process::Server, &path)?.read_all::<InputLogged>()?;
            if logged.iter().any(|input| input.stamp >= stamp) {
                return Ok(true);
            }
            thread::sleep(POLL);
        }
        Ok(false)
    }

    /// Waits until the server logged an input of the slot that `bot` learned it plays, or the
    /// deadline passed; whether it did.
    fn first_order_logged(&self, bot: Process, deadline: Instant) -> Result<bool, CheckError> {
        while Instant::now() < deadline {
            let started =
                ProcessLog::read(bot, &bot.log_path(self.dir))?.first::<MatchStarted>()?;
            if let Some(MatchStarted { slot, .. }) = started {
                let server =
                    ProcessLog::read(Process::Server, &Process::Server.log_path(self.dir))?;
                let logged = server.read_all::<InputLogged>()?;
                if logged.iter().any(|input| input.slot == slot) {
                    return Ok(true);
                }
            }
            thread::sleep(POLL);
        }
        Ok(false)
    }

    /// Plays a client bot of the first script alone on a local server with no network, a server
    /// bot of the second in slot 1, and waits until it ended or the deadline passed.
    pub(crate) fn play_local(&self) -> Result<Outcome, CheckError> {
        let deadline = Instant::now() + DEADLINE;
        let mut client = self.start(
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
        while Instant::now() < deadline {
            if let Some(status) = client.try_wait().map_err(|error| CheckError::Wait {
                process: Process::Local,
                error,
            })? {
                return Ok(Outcome::of(status));
            }
            thread::sleep(POLL);
        }
        stop(Process::Local, &mut client)
    }

    /// Starts `command` as `process`, in the run's directory, logging JSON to its file and text to
    /// a file beside it.
    fn start(&self, process: Process, command: &mut Command) -> Result<Child, CheckError> {
        let text = self.dir.join(format!("{}.log", process.file_stem()));
        #[expect(
            clippy::disallowed_methods,
            reason = "the LAN check writes its run directory, which a run of it reads and keeps"
        )]
        let text = File::create(&text).map_err(|error| CheckError::File { path: text, error })?;
        command
            .current_dir(self.dir)
            .env("CAMPFIRE_LOG", process.log_path(self.dir))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(text)
            .spawn()
            .map_err(|error| CheckError::Start { process, error })
    }
}

/// How each of `children` ended, waiting until each did or the deadline passed, when the check
/// stops each that still runs.
fn wait(children: &mut [(Process, Child)], deadline: Instant) -> Result<Vec<Outcome>, CheckError> {
    let mut outcomes = vec![None; children.len()];
    while outcomes.iter().any(Option::is_none) && Instant::now() < deadline {
        for ((process, child), outcome) in children.iter_mut().zip(&mut outcomes) {
            if outcome.is_none() {
                *outcome = child
                    .try_wait()
                    .map_err(|error| CheckError::Wait {
                        process: *process,
                        error,
                    })?
                    .map(Outcome::of);
            }
        }
        thread::sleep(POLL);
    }
    let mut ended = Vec::with_capacity(children.len());
    for ((process, child), outcome) in children.iter_mut().zip(outcomes) {
        ended.push(match outcome {
            Some(outcome) => outcome,
            None => stop(*process, child)?,
        });
    }
    Ok(ended)
}

/// Kills `child`, which overran the deadline.
fn stop(process: Process, child: &mut Child) -> Result<Outcome, CheckError> {
    kill(process, child)?;
    Ok(Outcome::Overran)
}

/// Kills `child`, as a crash ends a process.
fn kill(process: Process, child: &mut Child) -> Result<(), CheckError> {
    child
        .kill()
        .and_then(|()| child.wait())
        .map_err(|error| CheckError::Wait { process, error })?;
    Ok(())
}

/// A UDP port on `127.0.0.1` that no socket holds now.
fn free_port() -> Result<u16, CheckError> {
    UdpSocket::bind(("127.0.0.1", 0))
        .and_then(|socket| socket.local_addr())
        .map(|address| address.port())
        .map_err(|error| CheckError::Start {
            process: Process::Server,
            error,
        })
}
