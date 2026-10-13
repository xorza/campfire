//! The server's stop (design 10, Stop): a stop's signal ends the server by an exit with code 130,
//! whose endpoint closes each link, so a client hears it at once; a server whose start ignored
//! the terminal's signals still takes the stop's.

use std::path::Path;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use bevy_app::App;
use bevy_ecs::entity::Entity;
use campfire_log::LogLine;
use campfire_net::Listening;
use campfire_package::internals::TestMode;
use campfire_quic::{LINK_MIN_MTU, LinkEnd, LinkTimes, QuicEndpoint, QuicPlugin};
use campfire_store::internals::Scratch;
use campfire_store::{InputFile, StopSignals};
use lightyear::link::{Link, LinkMtu, LinkStart, Linked};

/// The longest each step waits, inside a test's bound of 1 s.
const PATIENCE: Duration = Duration::from_millis(800);

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
/// line the server writes meanwhile is cut, maybe within a character.
fn listening(log: &Path) -> Option<Listening> {
    let bytes = InputFile::read_if_present(log, LOG_MAX).unwrap()?;
    String::from_utf8_lossy(&bytes)
        .split_inclusive('\n')
        .filter(|line| line.ends_with('\n'))
        .find_map(|line| LogLine::parse(line).unwrap().read::<Listening>())
        .map(Result::unwrap)
}

#[test]
fn a_stop_closes_each_link_and_exits_with_its_code() {
    let scratch = Scratch::new();
    let log = scratch.path("log.jsonl");
    let mut command = Command::new(env!("CARGO_BIN_EXE_campfire-server"));
    command
        .env("RUST_LOG", "warn")
        .env("CAMPFIRE_LOG", &log)
        .env_remove("CAMPFIRE_LOG_FILTER")
        .env_remove("CAMPFIRE_LOG_FILTER_EXTRA")
        .arg("--data")
        .arg(scratch.path("data"))
        .arg("--no-port-map")
        .arg(TestMode::Lane.dir())
        .arg("127.0.0.1:0");
    let mut server = ServerProcess(
        StopSignals::ignoring_terminal(&mut command)
            .spawn()
            .unwrap(),
    );
    let listening = until("the server's start", || listening(&log));

    let mut client = ClientApp::connect(&listening);
    until("the link", || client.update().linked.then_some(()));
    StopSignals::stop(&server.0);
    // The server's close, not the idle timeout of 5 s, which `PATIENCE` does not reach.
    let end = until("the client's unlink", || client.update().end);
    assert_eq!(end, LinkEnd::PeerStopped);
    let status = until("the server's exit", || server.0.try_wait().unwrap());
    assert_eq!(status.code(), Some(i32::from(StopSignals::EXIT_CODE)));
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

/// A client's app, its entity the link and the endpoint.
#[derive(Debug)]
struct ClientApp {
    app: App,
    entity: Entity,
}

/// What a client's frame left: whether it is linked, and how its link ended.
#[derive(Debug)]
struct ClientState {
    linked: bool,
    end: Option<LinkEnd>,
}

impl ClientApp {
    /// A client that connects to the server `listening` names.
    fn connect(listening: &Listening) -> ClientApp {
        let quic = QuicEndpoint::client(listening.address, &listening.tls_key, LinkTimes::CAMPFIRE)
            .unwrap();
        let mut app = App::new();
        app.add_plugins(QuicPlugin);
        let entity = app
            .world_mut()
            .spawn((Link::default().with_mtu(LinkMtu::new(LINK_MIN_MTU)), quic))
            .id();
        app.world_mut().trigger(LinkStart { entity });
        ClientApp { app, entity }
    }

    /// Runs a frame.
    fn update(&mut self) -> ClientState {
        self.app.update();
        let world = self.app.world();
        ClientState {
            linked: world.get::<Linked>(self.entity).is_some(),
            end: world.get::<LinkEnd>(self.entity).copied(),
        }
    }
}
