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
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy_app::{App, AppExit, ScheduleRunnerPlugin, TaskPoolPlugin, Update};
use bevy_ecs::lifecycle::Add;
use bevy_ecs::message::MessageWriter;
use bevy_ecs::observer::On;
use bevy_ecs::query::{QueryState, With};
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::resource_exists;
use bevy_ecs::system::{Commands, Query, Res};
use bevy_ecs::world::World;
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_log::{LogEvent, Logging};
use campfire_net::{
    JournalFailed, Listening, MatchClock, NetProtocol, PlayerLink, SessionDir, SessionJournal,
    SessionWritten, SimServer,
};
use campfire_package::ModePackages;
use campfire_protocol::CertificateHash;
use campfire_protocol::secp256k1::{Keypair, Secp256k1};
use campfire_runner::Session;
use campfire_sim::TickRate;
use lightyear::prelude::server::{RawServer, ServerPlugins, Start, WebTransportServerIo};
use lightyear::prelude::{Connected, Identity, LinkOf, Linked, LocalAddr, ReplicationSender};
use tracing::{error, info};

use crate::data_dir::DataDir;
use crate::data_path::DataPath;
use crate::error::OpeningError;
use crate::opening::{Opening, OpeningSetup, Restore};
use crate::seated::Seated;
use crate::server_key::ServerKey;

mod data_dir;
mod data_path;
mod error;
mod opening;
mod seated;
mod server_key;

/// The process's exit code when the session's journal fails: sysexits' `EX_IOERR`, an error in
/// I/O on a file.
const JOURNAL_FAILED: u8 = 74;
/// The segments of a session's seed chain: the most checkpoints it may take, less one.
const SEGMENTS: NonZeroU32 = NonZeroU32::new(1024).unwrap();

/// How long after its journal's last write a session a stop ended restores, by default.
const RESTORE_WINDOW: Duration = Duration::from_secs(120);

const USAGE: &str = "usage: campfire-server --data <data directory> [--restore-window <seconds>] \
                     <mode package directory> <address, as 0.0.0.0:4433>";

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
        window,
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
    let opening = Opening::of(OpeningSetup {
        data: &data,
        packages,
        key: &server_key,
        certificate,
        window,
        segments: SEGMENTS,
        entropy: fill,
        clock: unix_now,
    });
    let opening = match opening {
        Ok(opening) => opening,
        Err(error @ (OpeningError::NewSession(_) | OpeningError::NewJournal(_))) => {
            JournalFailed {
                error: error.to_string(),
            }
            .log();
            return ExitCode::from(JOURNAL_FAILED);
        }
        Err(error) => {
            error!(data = %data.display(), %error, "no session starts");
            return ExitCode::FAILURE;
        }
    };
    let listening = announce(&opening, &mode, address, certificate, &server_key);
    let tick = TickRate::new(opening.terms().tick_hz).length();
    let mut app = server_app(
        opening,
        ServerKey(server_key),
        DataPath(data),
        tick,
        listening,
    );
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

/// The server's app: its plugins at `tick` a tick, the session of `opening`, a `listening`
/// event logged once its transport listens, and the end once every player left.
fn server_app(
    opening: Opening,
    key: ServerKey,
    data: DataPath,
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
    match opening {
        Opening::New(lobby) => app.insert_resource(*lobby),
        Opening::Restored(restore) => app.insert_resource(*restore),
    };
    app.insert_resource(key);
    app.insert_resource(data);
    app.add_observer(
        |added: On<'_, '_, Add, LinkOf>, mut commands: Commands<'_, '_>| {
            commands.entity(added.entity).insert(ReplicationSender);
        },
    );
    app.add_observer(
        |_: On<'_, '_, Add, PlayerLink>, mut commands: Commands<'_, '_>| {
            commands.insert_resource(Seated);
        },
    );
    app.add_observer(
        move |added: On<'_, '_, Add, Linked>, servers: Query<'_, '_, (), With<RawServer>>| {
            if servers.contains(added.entity) {
                listening.log();
            }
        },
    );
    app.add_systems(
        Update,
        (
            Restore::run.run_if(resource_exists::<Restore>),
            exit_on_journal_failure,
            end_when_everyone_left,
        )
            .chain(),
    );
    app
}

/// What the server logs once its transport listens for the session `opening` starts: how a
/// player joins it, through `address` and the certificate of `certificate`'s hash; and logs now,
/// for a new session, that it opened, of the mode `mode`.
fn announce(
    opening: &Opening,
    mode: &Path,
    address: SocketAddr,
    certificate: CertificateHash,
    key: &Keypair,
) -> Listening {
    let terms = opening.terms();
    if let Opening::New(_) = opening {
        info!(
            session = %terms.session_id(),
            %address,
            players = terms.slots.len(),
            mode = %mode.display(),
            "opened a session"
        );
    }
    let key = key.x_only_public_key().0;
    let tick_hz = terms.tick_hz;
    Listening {
        certificate,
        server_key: key,
        tick_hz,
        join: format!(
            "campfire-client {} <this machine's LAN address>:{} {certificate} {key} {tick_hz}",
            mode.display(),
            address.port()
        ),
    }
}

/// What the command line names: the data directory, how long after its journal's last write a
/// session a stop ended restores, the mode to open, and the address to listen on.
#[derive(Debug)]
struct Args {
    data: PathBuf,
    window: Duration,
    mode: PathBuf,
    address: SocketAddr,
}

impl Args {
    fn parse(args: impl Iterator<Item = OsString>) -> Result<Args, String> {
        let mut args = args.peekable();
        let (Some(flag), Some(data)) = (args.next(), args.next()) else {
            return Err("--data and its directory are needed".to_owned());
        };
        if flag != "--data" {
            return Err(format!("{}: not --data", flag.display()));
        }
        let mut window = RESTORE_WINDOW;
        if args.next_if(|arg| arg == "--restore-window").is_some() {
            let seconds = args.next().ok_or("--restore-window needs its seconds")?;
            window = seconds
                .to_str()
                .and_then(|text| text.parse().ok())
                .map(Duration::from_secs)
                .ok_or_else(|| format!("{}: not a whole number of seconds", seconds.display()))?;
        }
        let (Some(mode), Some(address), None) = (args.next(), args.next(), args.next()) else {
            return Err("the mode and the address are needed, and nothing after".to_owned());
        };
        let address = address
            .to_str()
            .and_then(|text| text.parse::<SocketAddr>().ok())
            .ok_or_else(|| format!("{}: not a socket address", address.display()))?;
        Ok(Args {
            data: PathBuf::from(data),
            window,
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

/// Once the match started, a player took a slot since the server started, and no player is
/// connected any more, ends the session with its result,
/// as the mode ended the match or aborted when it did not, reveals the seed, writes the session
/// log durably into the data directory's `logs` and exits: with an error when the log is not
/// written, as the session it holds is lost.
fn end_when_everyone_left(
    world: &mut World,
    connected: &mut QueryState<(), (With<PlayerLink>, With<Connected>)>,
) {
    if !world.contains_resource::<MatchClock>() || !world.contains_resource::<Seated>() {
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
    let id = session.log().session_id();
    let data = &world.resource::<DataPath>().0;
    let exit = match SessionDir::publish(data, session.log()) {
        Ok(file) => {
            SessionWritten {
                session: id,
                file,
                hash,
            }
            .log();
            AppExit::Success
        }
        Err(error) => {
            error!(session = %id, %error, "could not write the session log");
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

    use campfire_protocol::SeedChain;
    use campfire_protocol::secp256k1::SecretKey;
    use campfire_protocol::{Journal, JournalFile, SessionHeader, SessionLog, SlotPlan, SlotStart};
    use campfire_runner::{InputRules, SessionRules};

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
