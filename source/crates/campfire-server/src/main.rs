//! Headless game server: opens a session of a mode on an address, lets its players join over
//! WebTransport, runs the match, and writes the session log once every player left. Its data
//! directory, which it holds locked while it runs, keeps its key.
//!
//! Logs go to standard error, filtered by `RUST_LOG` (`info` by default). With `CAMPFIRE_LOG` set
//! to a path, they also go there as JSON lines, filtered by `CAMPFIRE_LOG_FILTER` (Campfire's
//! `debug` by default).

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use std::env;
use std::ffi::OsString;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy_app::{App, AppExit, ScheduleRunnerPlugin, TaskPoolPlugin, Update};
use bevy_ecs::lifecycle::Add;
use bevy_ecs::message::MessageWriter;
use bevy_ecs::observer::On;
use bevy_ecs::query::{QueryState, With};
use bevy_ecs::system::{Commands, Query, Res};
use bevy_ecs::world::World;
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_log::{LogEvent, Logging};
use campfire_net::{
    JournalFailed, Listening, Lobby, LobbySetup, MatchClock, NetProtocol, PlayerLink,
    SessionJournal, SessionWritten, SimServer,
};
use campfire_package::ModePackages;
use campfire_protocol::secp256k1::Secp256k1;
use campfire_protocol::{CertificateHash, DurableFile, SeedChain, SessionPrivate};
use campfire_runner::{InputRules, Session};
use campfire_sim::TickRate;
use lightyear::prelude::server::{RawServer, ServerPlugins, Start, WebTransportServerIo};
use lightyear::prelude::{Connected, Identity, LinkOf, Linked, LocalAddr, ReplicationSender};
use tracing::{error, info};

use crate::data_dir::DataDir;
use crate::logs_dir::LogsDir;
use crate::server_key::ServerKey;
use crate::session_dir::SessionDir;

mod data_dir;
mod logs_dir;
mod server_key;
mod session_dir;

/// The process's exit code when the session's journal fails: sysexits' `EX_IOERR`, an error in
/// I/O on a file.
const JOURNAL_FAILED: u8 = 74;
/// The segments of a session's seed chain: the most checkpoints it may take, less one.
const SEGMENTS: NonZeroU32 = NonZeroU32::new(1024).unwrap();

const USAGE: &str = "usage: campfire-server --data <data directory> <mode package directory> \
                     <address, as 0.0.0.0:4433>";

/// How often the app loop runs: often enough that no fixed tick waits long for its frame.
const FRAME: Duration = Duration::from_millis(2);

/// What the terminal shows when `RUST_LOG` does not say.
const TERMINAL_FILTER: &str = "info";
/// What the log file holds when `CAMPFIRE_LOG_FILTER` does not say: Campfire's messages down to
/// `debug`, and Lightyear's rollbacks; of the rest, `info` and above.
const FILE_FILTER: &str = "info,campfire_server=debug,campfire_net=debug,campfire_runner=debug,\
                           campfire_script=debug,lightyear_prediction=debug";

fn main() -> ExitCode {
    Logging {
        terminal: TERMINAL_FILTER,
        file: FILE_FILTER,
    }
    .start();
    let Args {
        data,
        mode,
        address,
    } = match Args::parse(env::args_os().skip(1)) {
        Ok(args) => args,
        Err(problem) => {
            error!(%problem, USAGE);
            return ExitCode::from(2);
        }
    };
    let data_dir = match DataDir::open(&data, fill) {
        Ok(data_dir) => data_dir,
        Err(error) => {
            error!(data = %data.display(), %error, "the data directory does not open");
            return ExitCode::FAILURE;
        }
    };
    let packages = match ModePackages::from_dir(&mode) {
        Ok(packages) => packages,
        Err(error) => {
            error!(mode = %mode.display(), %error, "the mode does not load");
            return ExitCode::FAILURE;
        }
    };
    let identity = Identity::self_signed(["localhost"]).expect("a fixed name is a valid SAN");
    let certificate =
        CertificateHash::new(*identity.certificate_chain().as_slice()[0].hash().as_ref());
    let server_key = data_dir.key;
    let players = usize::try_from(packages.manifest().slots()).expect("the slots fit usize");
    let key = server_key.x_only_public_key().0;
    let tick_hz = packages.manifest().tick_hz.default();
    let seed_chain = SeedChain::new(random(), SEGMENTS);
    let mut lobby = Lobby::new(LobbySetup {
        packages,
        server_key: key,
        seed_chain,
        tick_hz,
        inputs: InputRules::LAN,
        certificate,
        players,
        clock: unix_now,
        entropy: fill,
    })
    .expect("a mode runs at its default rate");
    let private = SessionPrivate {
        seed_chain,
        terms: lobby.terms().clone(),
    };
    let journal = SessionDir::create(&data, &private)
        .map_err(|error| error.to_string())
        .and_then(|dir| dir.start_journal().map_err(|error| error.to_string()));
    match journal {
        Ok(journal) => lobby.keep_journal(journal),
        Err(error) => {
            JournalFailed { error }.log();
            return ExitCode::from(JOURNAL_FAILED);
        }
    }
    let tick = TickRate::new(tick_hz).length();

    info!(
        session = %lobby.terms().session_id(),
        %address,
        players,
        mode = %mode.display(),
        "opened a session"
    );

    let listening = Listening {
        certificate,
        server_key: key,
        tick_hz,
        join: format!(
            "campfire-client {} <this machine's LAN address>:{} {certificate} {key} {tick_hz}",
            mode.display(),
            address.port()
        ),
    };
    let logs = LogsDir(data.join("logs"));
    let mut app = server_app(lobby, ServerKey(server_key), logs, tick, listening);
    let server = app
        .world_mut()
        .spawn((
            RawServer,
            WebTransportServerIo {
                certificate: identity,
            },
            LocalAddr(address),
        ))
        .id();
    app.world_mut().trigger(Start { entity: server });
    exit_code(app.run())
}

/// The server's app: its plugins at `tick` a tick, the session `lobby` opened, a `listening`
/// event logged once its transport listens, and the end once every player left.
fn server_app(
    lobby: Lobby,
    key: ServerKey,
    logs: LogsDir,
    tick: Duration,
    listening: Listening,
) -> App {
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        TimePlugin,
        StatesPlugin,
        ScheduleRunnerPlugin::run_loop(FRAME),
    ));
    app.add_plugins(ServerPlugins {
        tick_duration: tick,
    });
    app.add_plugins((NetProtocol, SimServer));
    app.insert_resource(lobby);
    app.insert_resource(key);
    app.insert_resource(logs);
    app.add_observer(
        |added: On<'_, '_, Add, LinkOf>, mut commands: Commands<'_, '_>| {
            commands.entity(added.entity).insert(ReplicationSender);
        },
    );
    app.add_observer(
        move |added: On<'_, '_, Add, Linked>, servers: Query<'_, '_, (), With<RawServer>>| {
            if servers.contains(added.entity) {
                listening.log();
            }
        },
    );
    app.add_systems(Update, (exit_on_journal_failure, end_when_everyone_left));
    app
}

/// What the command line names: the data directory, the mode to open, and the address to listen
/// on.
#[derive(Debug)]
struct Args {
    data: PathBuf,
    mode: PathBuf,
    address: SocketAddr,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Args, String> {
        let (Some(flag), Some(data), Some(mode), Some(address), None) = (
            args.next(),
            args.next(),
            args.next(),
            args.next(),
            args.next(),
        ) else {
            return Err("four arguments are needed".to_owned());
        };
        if flag != "--data" {
            return Err(format!("{}: not --data", flag.display()));
        }
        let address = address
            .to_str()
            .and_then(|text| text.parse::<SocketAddr>().ok())
            .ok_or_else(|| format!("{}: not a socket address", address.display()))?;
        Ok(Args {
            data: PathBuf::from(data),
            mode: PathBuf::from(mode),
            address,
        })
    }
}

/// The process's exit code for how the app exited.
fn exit_code(exit: AppExit) -> ExitCode {
    match exit {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(code) => ExitCode::from(code.get()),
    }
}

/// Exits, with `JOURNAL_FAILED`, once a write or a sync of the session's journal failed: the
/// server keeps no record past it, and its host's supervisor starts it again.
fn exit_on_journal_failure(
    journal: Option<Res<'_, SessionJournal>>,
    mut exit: MessageWriter<'_, AppExit>,
) {
    let Some(failure) = journal.and_then(|journal| journal.0.take_failure()) else {
        return;
    };
    JournalFailed {
        error: failure.to_string(),
    }
    .log();
    exit.write(AppExit::from_code(JOURNAL_FAILED));
}

/// Once the match started and no player is connected any more, ends the session with its result,
/// as the mode ended the match or aborted when it did not, reveals the seed, writes the session
/// log durably into the data directory's `logs` and exits: with an error when the log is not
/// written, as the session it holds is lost.
fn end_when_everyone_left(
    world: &mut World,
    connected: &mut QueryState<(), (With<PlayerLink>, With<Connected>)>,
) {
    if !world.contains_resource::<MatchClock>() {
        return;
    }
    if connected.iter(world).next().is_some() {
        return;
    }
    let session = world.resource::<Session>();
    let result = session.result(world, Session::outcome(world));
    let id = session.log().session_id();
    let key = &world.resource::<ServerKey>().0;
    let signature = result.sign(&Secp256k1::new(), key, id, &random());
    let mut session = world.resource_mut::<Session>();
    session
        .record_result(result, &signature)
        .expect("the server's own result holds");
    session.reveal_seed();
    let session = world.resource::<Session>();
    let hash = session.state_hash(world);
    let mut bytes = Vec::new();
    session.log().encode(&mut bytes);
    let id = session.log().header().terms.session_id();
    let logs = &world.resource::<LogsDir>().0;
    let file = logs.join(format!("{id}.campfire-log"));
    let written = DurableFile::create_dir(logs).and_then(|()| DurableFile::write(&file, &bytes));
    let exit = match written {
        Ok(()) => {
            SessionWritten {
                session: id,
                file,
                hash,
            }
            .log();
            AppExit::Success
        }
        Err(error) => {
            error!(file = %file.display(), %error, "could not write the session log");
            AppExit::error()
        }
    };
    world.write_message(exit);
}

fn random() -> [u8; 32] {
    let mut bytes = [0; 32];
    fill(&mut bytes);
    bytes
}

fn fill(bytes: &mut [u8; 32]) {
    getrandom::fill(bytes).expect("the OS gives random bytes");
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::num::NonZeroU8;
    use std::path::Path;

    use campfire_protocol::secp256k1::{Keypair, SecretKey};
    use campfire_protocol::{Journal, JournalFile, SessionHeader, SessionLog, SlotPlan, SlotStart};
    use campfire_runner::SessionRules;

    use super::*;

    /// A journal's file whose every sync fails.
    #[derive(Debug)]
    struct FailingFile;

    impl JournalFile for FailingFile {
        fn append(&mut self, _: &[u8]) -> io::Result<()> {
            Ok(())
        }

        fn sync(&mut self) -> io::Result<()> {
            Err(io::Error::other("sync failed"))
        }
    }

    #[test]
    fn a_failed_journal_ends_the_server_with_its_exit_code() {
        // A session of the test lane mode with one open slot, so its header needs no player; the
        // log's header is its journal's first record, whose sync fails.
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/test/modes/lane"
        );
        let packages = ModePackages::from_dir(Path::new(dir)).unwrap();
        let secret = SecretKey::from_byte_array(&[8; 32]).unwrap();
        let key = Keypair::from_secret_key(&Secp256k1::new(), &secret);
        let terms = SessionRules::of(&packages)
            .terms(
                key.x_only_public_key().0,
                SeedChain::new([9; 32], NonZeroU32::MIN).commitment(),
                packages.manifest().tick_hz.default(),
                InputRules::LAN,
                vec![SlotPlan::Open],
            )
            .unwrap();
        let header = SessionHeader {
            terms,
            slots: vec![SlotStart::Open],
        };
        let mut log = SessionLog::new(header).unwrap();
        let journal = Journal::start(FailingFile);
        let mut app = App::new();
        app.insert_resource(SessionJournal(journal.watch()));
        app.add_systems(Update, exit_on_journal_failure);
        app.update();
        assert_eq!(app.should_exit(), None);
        log.keep_journal(journal);
        // Dropped, the log's journal waits for its writer, which stopped at the failed sync.
        drop(log);
        app.update();
        let code = NonZeroU8::new(JOURNAL_FAILED).unwrap();
        assert_eq!(app.should_exit(), Some(AppExit::Error(code)));
    }
}
