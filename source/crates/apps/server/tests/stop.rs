//! The server's stop (08-sessions.md, A stop is an exit, not a crash): a stop's signal ends the
//! server by an exit with code 130, whose endpoint closes each link, so a client hears it at once;
//! a server whose start ignored the terminal's signals still takes the stop's.

use std::path::Path;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use campfire_common::ExitStatus;
use campfire_log::LogLine;
use campfire_net::internals::ProtocolClient;
use campfire_net::{Listening, UdpLinks};
use campfire_net_endpoint::Endpoint;
use campfire_net_payload::{AppCode, UnlinkReason};
use campfire_net_socket::ClientSocket;
use campfire_protocol::SessionId;
use campfire_store::internals::TestDir;
use campfire_store::{InputFile, StopSignals};
use campfire_test_packages::TestMode;

/// The longest each step after the server's start waits, inside a test's bound of 1 s.
const PATIENCE: Duration = Duration::from_millis(800);

/// The mode the server opens, whose protocol the client takes.
const MODE: TestMode = TestMode::Lane;

/// Far more than the few lines of the server's start.
const LOG_MAX: usize = 1 << 20;

/// Calls `step` until it gives a value, a millisecond apart.
fn until<T>(what: &str, mut step: impl FnMut() -> Option<T>) -> T {
    let start = Instant::now();
    loop {
        if let Some(value) = step() {
            return value;
        }
        assert!(start.elapsed() < PATIENCE, "{what} took too long");
        thread::sleep(Duration::from_millis(1));
    }
}

/// The `Listening` event among the whole lines of the JSON log at `log`, once it holds one; the
/// line the server writes meanwhile is cut, maybe within a character. The server logs it inside
/// the root span of its session, which names the session by its id.
fn listening(log: &Path) -> Option<Listening> {
    let bytes = InputFile::read_if_present(log, LOG_MAX).unwrap()?;
    String::from_utf8_lossy(&bytes)
        .split_inclusive('\n')
        .filter(|line| line.ends_with('\n'))
        .find_map(|line| {
            let line = LogLine::parse(line).unwrap();
            let listening = line.read::<Listening>()?.unwrap();
            let session = line.span_text("session", "id");
            assert!(
                session.is_some_and(|id| id.parse::<SessionId>().is_ok()),
                "{session:?}"
            );
            Some(listening)
        })
}

#[test]
fn a_stop_closes_each_link_and_exits_with_its_code() {
    let test_dir = TestDir::new();
    let log = test_dir.path("log.jsonl");
    let mut command = Command::new(env!("CARGO_BIN_EXE_campfire-server"));
    command
        .env("RUST_LOG", "warn")
        .env("CAMPFIRE_LOG", &log)
        .env_remove("CAMPFIRE_LOG_FILTER")
        .env_remove("CAMPFIRE_LOG_FILTER_EXTRA")
        .arg("--data")
        .arg(test_dir.path("data"))
        .arg("--no-port-map")
        .arg(MODE.dir())
        .arg("127.0.0.1:0");
    let mut server = ServerProcess(
        StopSignals::ignoring_terminal(&mut command)
            .spawn()
            .unwrap(),
    );
    // The start has no bound of its own: on macOS, the first start of a new binary waits for
    // XProtect's scan of it, which scans one executable at a time, after the test binaries that
    // nextest starts. Nextest's bound of the test ends a start that hangs.
    let listening = loop {
        if let Some(listening) = listening(&log) {
            break listening;
        }
        if let Some(status) = server.0.try_wait().unwrap() {
            panic!("the server exited before it listened: {status}");
        }
        thread::sleep(Duration::from_millis(1));
    };

    let socket =
        ClientSocket::bind(listening.address, UdpLinks::client(listening.tls_key)).unwrap();
    let capabilities = MODE.packages().manifest().capabilities;
    let mut client =
        ProtocolClient::connect(Endpoint::from(socket.endpoint()), &listening, capabilities);
    until("the session's offer", || {
        let state = client.update();
        (state.linked && state.offered).then_some(())
    });
    StopSignals::stop(&server.0);
    // The server's close, not the idle timeout of 5 s, which `PATIENCE` does not reach.
    let end = until("the client's unlink", || client.update().end);
    assert_eq!(end, UnlinkReason::ClosedByPeer(AppCode::DROPPED));
    let status = until("the server's exit", || server.0.try_wait().unwrap());
    assert_eq!(
        status.code(),
        Some(i32::from(ExitStatus::Interrupted.code()))
    );
}

/// The server's process, killed if the test fails before it exits.
#[derive(Debug)]
struct ServerProcess(Child);

impl Drop for ServerProcess {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            self.0.kill().unwrap();
            self.0.wait().unwrap();
        }
    }
}
