use std::fs::File;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use campfire_common::Bytes32;
use campfire_net::Listening;

use crate::binaries::Binaries;
use crate::error::CheckError;
use crate::outcome::Outcome;
use crate::process::Process;
use crate::process_log::ProcessLog;

/// How long the server gets to listen, and then the whole match to end, before the check stops
/// what still runs: a match of 3 s ends in a few more.
const DEADLINE: Duration = Duration::from_secs(30);
/// How often the check looks at the processes while it waits.
const POLL: Duration = Duration::from_millis(50);
/// The server's data directory, in the run's directory.
pub(crate) const SERVER_DATA: &str = "server-data";

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

/// How each process of a match ended.
#[derive(Debug)]
pub(crate) struct Played {
    pub(crate) server: Outcome,
    pub(crate) bots: Vec<Outcome>,
    pub(crate) impostor: Outcome,
}

impl LanMatch<'_> {
    /// Starts the server, waits until it listens, starts the bots and the impostor, and waits until
    /// every process ended or the deadline passed.
    pub(crate) fn play(&self) -> Result<Played, CheckError> {
        let deadline = Instant::now() + DEADLINE;
        let address = SocketAddr::from(([127, 0, 0, 1], free_port()?));
        let mut server = self.start(
            Process::Server,
            Command::new(&self.binaries.server)
                .arg("--data")
                .arg(self.dir.join(SERVER_DATA))
                .arg(self.mode)
                .arg(address.to_string()),
        )?;
        let log = Process::Server.log_path(self.dir);
        let listening = loop {
            let listening = ProcessLog::read(Process::Server, &log)?.first::<Listening>()?;
            if listening.is_some() || Instant::now() >= deadline {
                break listening;
            }
            if let Some(status) = server.try_wait().map_err(|error| CheckError::Wait {
                process: Process::Server,
                error,
            })? {
                return Ok(Played {
                    server: Outcome::of(status),
                    bots: vec![Outcome::NotStarted; self.scripts.len()],
                    impostor: Outcome::NotStarted,
                });
            }
            thread::sleep(POLL);
        };
        let Some(Listening {
            certificate,
            server_key,
            tick_hz,
            ..
        }) = listening
        else {
            return Ok(Played {
                server: stop(Process::Server, &mut server)?,
                bots: vec![Outcome::NotStarted; self.scripts.len()],
                impostor: Outcome::NotStarted,
            });
        };
        let mut children = vec![(Process::Server, server)];
        let wrong = Bytes32::new(certificate.as_bytes().map(|byte| !byte)).to_string();
        let bots = self
            .scripts
            .iter()
            .enumerate()
            .map(|(index, script)| (Process::Bot(index), script, certificate.to_string()));
        let impostor = (Process::Impostor, &self.scripts[0], wrong);
        for (process, script, certificate) in bots.chain([impostor]) {
            let bot = self.start(
                process,
                Command::new(&self.binaries.client)
                    .arg("--bot")
                    .arg(script)
                    .arg("--key")
                    .arg(self.dir.join(format!("{}.nsec", process.file_stem())))
                    .arg(self.mode)
                    .arg(address.to_string())
                    .arg(certificate)
                    .arg(server_key.to_string())
                    .arg(tick_hz.to_string()),
            )?;
            children.push((process, bot));
        }
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
        let server = ended.remove(0);
        let impostor = ended.pop().expect("the impostor ran");
        Ok(Played {
            server,
            bots: ended,
            impostor,
        })
    }

    /// Starts `command` as `process`, in the run's directory, logging JSON to its file and text to
    /// a file beside it.
    fn start(&self, process: Process, command: &mut Command) -> Result<Child, CheckError> {
        let text = self.dir.join(format!("{}.log", process.file_stem()));
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

/// Kills `child`, which overran the deadline.
fn stop(process: Process, child: &mut Child) -> Result<Outcome, CheckError> {
    child
        .kill()
        .and_then(|()| child.wait())
        .map_err(|error| CheckError::Wait { process, error })?;
    Ok(Outcome::Overran)
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
