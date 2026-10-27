use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::future;
use std::mem;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use campfire_codec::Json;
use campfire_common::ErrorReport;
use campfire_console::{
    ConsoleAnswer, ConsoleAsk, ConsoleEvent, ConsoleOpening, EventLevel, Output, Reply, Request,
    RequestId,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{self, Notify, oneshot};
use tokio::time;
use tracing::{info, warn};

use crate::error::GameFailure;
use crate::process_exit::ProcessExit;

/// A game this server started, with its console on the game's standard streams: the requests it
/// sends, each answered by id; the latest events of the game's log, in a ring; and how the game
/// ended, once it did (console.md, A game's end answers its calls).
#[derive(Debug)]
pub(crate) struct GameProcess {
    input: sync::Mutex<Option<ChildStdin>>,
    state: Arc<Mutex<GameState>>,
    ended: Arc<Notify>,
    kill: Mutex<Option<oneshot::Sender<()>>>,
    next: AtomicU64,
}

/// A game that started, and its opening line.
#[derive(Debug)]
pub(crate) struct StartedGame {
    pub(crate) game: GameProcess,
    pub(crate) opening: ConsoleOpening,
}

/// What a game's reader and its callers share.
#[derive(Debug, Default)]
struct GameState {
    opening: Option<oneshot::Sender<ConsoleOpening>>,
    pending: BTreeMap<RequestId, oneshot::Sender<Result<Reply, GameFailure>>>,
    events: VecDeque<ConsoleEvent>,
    end: Option<GameEnd>,
}

/// How a game ended: its exit status, and its log's last errors.
#[derive(Debug, Clone)]
struct GameEnd {
    status: ProcessExit,
    errors: String,
}

/// Which of a game's events a read takes: at a level or above, of a target's prefix, of a tick
/// or after, the latest `limit`.
#[derive(Debug, Clone)]
pub(crate) struct EventFilter {
    pub(crate) level: EventLevel,
    pub(crate) target: Option<String>,
    pub(crate) since_tick: Option<u64>,
    pub(crate) limit: usize,
}

impl GameProcess {
    /// The most events the ring keeps.
    const EVENTS: usize = 4096;

    /// The errors of the log a failure of an exited game names.
    const LAST_ERRORS: usize = 5;

    /// Starts `binary` with `args` and its console on its standard streams, and gives it once its
    /// opening line came; a game that exits before fails with its exit status.
    pub(crate) async fn start(
        binary: &Path,
        args: &[OsString],
    ) -> Result<StartedGame, GameFailure> {
        let mut child = Command::new(binary)
            .args(args)
            .arg("--console")
            .arg("stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(GameFailure::Spawn)?;
        let input = child.stdin.take().expect("a piped input");
        let output = child.stdout.take().expect("a piped output");
        let (opened, opening) = oneshot::channel();
        let state = Arc::new(Mutex::new(GameState {
            opening: Some(opened),
            ..GameState::default()
        }));
        let ended = Arc::new(Notify::new());
        let (kill, killed) = oneshot::channel();
        tokio::spawn(Self::read(
            child,
            output,
            killed,
            Arc::clone(&state),
            Arc::clone(&ended),
        ));
        let game = Self {
            input: sync::Mutex::new(Some(input)),
            state,
            ended,
            kill: Mutex::new(Some(kill)),
            next: AtomicU64::new(1),
        };
        match opening.await {
            Ok(opening) => Ok(StartedGame { game, opening }),
            Err(_) => Err(game.failure()),
        }
    }

    /// Sends `ask` and gives the game's reply; a refusal fails, and so does a game that exited.
    pub(crate) async fn ask(&self, ask: ConsoleAsk) -> Result<Reply, GameFailure> {
        self.ask_until(ask, future::pending()).await
    }

    /// Sends `ask` and gives the game's reply, as `ask` does, until `cancelled` comes: the game
    /// is then asked to end the run `ask` started, and the reply is given up, as MCP's cancel
    /// asks (console.md, A cancel pauses the run).
    pub(crate) async fn ask_until(
        &self,
        ask: ConsoleAsk,
        cancelled: impl Future<Output = ()>,
    ) -> Result<Reply, GameFailure> {
        let id = RequestId(self.next.fetch_add(1, Ordering::Relaxed));
        let (answered, reply) = oneshot::channel();
        {
            let mut state = self.state();
            if state.end.is_some() {
                drop(state);
                return Err(self.failure());
            }
            state.pending.insert(id, answered);
        }
        self.send(&Request { id, ask }).await;
        let reply = tokio::select! {
            reply = reply => reply,
            () = cancelled => {
                let cancel = RequestId(self.next.fetch_add(1, Ordering::Relaxed));
                self.send(&Request { id: cancel, ask: ConsoleAsk::Cancel(id) }).await;
                self.state().pending.remove(&id);
                return Err(GameFailure::Cancelled);
            }
        };
        let reply = reply.unwrap_or_else(|_| Err(self.failure()))?;
        match reply.answer {
            ConsoleAnswer::Refused(refusal) => Err(GameFailure::Refused(refusal)),
            _ => Ok(reply),
        }
    }

    /// The latest events `filter` takes, the oldest first.
    pub(crate) fn events(&self, filter: &EventFilter) -> Vec<ConsoleEvent> {
        let state = self.state();
        let takes = |event: &&ConsoleEvent| {
            event.level >= filter.level
                && (filter.target.as_ref()).is_none_or(|target| event.target.starts_with(target))
                && (filter.since_tick)
                    .is_none_or(|since| event.tick.is_some_and(|tick| tick >= since))
        };
        let mut taken: Vec<ConsoleEvent> = state
            .events
            .iter()
            .rev()
            .filter(takes)
            .take(filter.limit)
            .cloned()
            .collect();
        taken.reverse();
        taken
    }

    /// Stops the game as MCP's stdio stops a server: its input closed, a wait of `patience`,
    /// then the OS's end; gives its exit status.
    pub(crate) async fn stop(&self, patience: Duration) -> ProcessExit {
        self.input.lock().await.take();
        if time::timeout(patience, self.until_ended()).await.is_err() {
            if let Some(kill) = self
                .kill
                .lock()
                .expect("no code panics holding a game's kill")
                .take()
            {
                kill.send(()).ok();
            }
            self.until_ended().await;
        }
        self.state()
            .end
            .clone()
            .map_or(ProcessExit::Running, |end| end.status)
    }

    /// Waits for the game's end.
    async fn until_ended(&self) {
        loop {
            let notified = self.ended.notified();
            if self.state().end.is_some() {
                return;
            }
            notified.await;
        }
    }

    /// Writes `request` to the game's input; a game that closed its input fails the request as
    /// it ends.
    async fn send(&self, request: &Request) {
        let mut line = Json::write(request);
        line.push('\n');
        let mut input = self.input.lock().await;
        if let Some(input) = input.as_mut() {
            input.write_all(line.as_bytes()).await.ok();
            input.flush().await.ok();
        }
    }

    /// Reads the game's output to its end: the opening, each reply to its request, each event
    /// into the ring; a stray line, as a dependency's print, goes to the log (console.md, A line
    /// that is no reply is logged). At the end, the game's exit fails each request it did not
    /// answer.
    async fn read(
        mut child: Child,
        output: ChildStdout,
        mut killed: oneshot::Receiver<()>,
        state: Arc<Mutex<GameState>>,
        ended: Arc<Notify>,
    ) {
        let mut lines = BufReader::new(output).lines();
        let mut kill_sent = false;
        loop {
            let line = tokio::select! {
                line = lines.next_line() => line,
                _ = &mut killed, if !kill_sent => {
                    kill_sent = true;
                    child.start_kill().ok();
                    continue;
                }
            };
            let Ok(Some(line)) = line else {
                break;
            };
            let mut state = state.lock().expect("no code panics holding a game's state");
            match Json::parse::<Output>(&line) {
                Ok(Output::Opening(opening)) => {
                    if let Some(opened) = state.opening.take() {
                        opened.send(opening).ok();
                    }
                }
                Ok(Output::Reply(reply)) => {
                    if let Some(answered) = state.pending.remove(&reply.id) {
                        answered.send(Ok(reply)).ok();
                    }
                }
                Ok(Output::Event(event)) => {
                    if state.events.len() == Self::EVENTS {
                        state.events.pop_front();
                    }
                    state.events.push_back(event);
                }
                Err(_) => info!(line, "the game wrote a line that is no console's"),
            }
        }
        let status = match child.wait().await {
            Ok(status) => status.into(),
            Err(error) => {
                warn!(error = %ErrorReport::of(&error), "the game's exit status does not read");
                ProcessExit::Unread
            }
        };
        let mut state = state.lock().expect("no code panics holding a game's state");
        let mut errors: Vec<String> = (state.events.iter().rev())
            .filter(|event| event.level == EventLevel::Error)
            .take(Self::LAST_ERRORS)
            .map(|event| {
                format!(
                    "{}: {}",
                    event.target,
                    event.message.as_deref().unwrap_or("")
                )
            })
            .collect();
        errors.reverse();
        let end = GameEnd {
            status,
            errors: errors.join("\n"),
        };
        for (_, answered) in mem::take(&mut state.pending) {
            answered
                .send(Err(GameFailure::Exited {
                    status: end.status,
                    errors: end.errors.clone(),
                }))
                .ok();
        }
        state.opening = None;
        state.end = Some(end);
        drop(state);
        ended.notify_waiters();
    }

    fn state(&self) -> MutexGuard<'_, GameState> {
        self.state
            .lock()
            .expect("no code panics holding a game's state")
    }

    /// The failure of a call of the game that ended.
    fn failure(&self) -> GameFailure {
        let end = self.state().end.clone().unwrap_or_else(|| GameEnd {
            status: ProcessExit::Running,
            errors: String::new(),
        });
        GameFailure::Exited {
            status: end.status,
            errors: end.errors,
        }
    }
}
