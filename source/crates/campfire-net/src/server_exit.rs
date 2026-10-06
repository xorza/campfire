use bevy_app::AppExit;
use bevy_ecs::world::World;
use campfire_log::LogEvent;
use campfire_runner::Session;
use tracing::error;

use crate::events::session_written::SessionWritten;
use crate::faults::Faults;
use crate::faults::fault::Fault;
use crate::match_clock::MatchClock;
use crate::server_data::ServerData;
use crate::session_dir::SessionDir;
use crate::sim_server::SimServer;

/// When a session's server exits, a dedicated one or a local one.
#[derive(Debug)]
pub struct ServerExit;

impl ServerExit {
    /// The exit code of a server whose journal or checkpoint's snapshot failed, which keeps no
    /// record past it: sysexits' `EX_IOERR`, an error in I/O on a file. A dedicated server's
    /// supervisor then starts it again, which restores the session.
    pub const STORAGE_FAILED: u8 = 74;

    /// How the server of the session in `world` exits in this frame, when it does: as `Faults`
    /// says, once a worker failed, which for its journal or a checkpoint's snapshot is with
    /// `STORAGE_FAILED`; before the match started, at once when `stop` asks it to. Once the match
    /// started, when `stop` asks it to or every player left, it ends the session, as the mode
    /// ended the match or aborted; it then publishes the log in its `ServerData`, and exits, with
    /// an error when the log is not written, as the session it holds is lost.
    pub fn due(world: &mut World, stop: bool) -> Option<AppExit> {
        if let Some(exit) = Faults::apply(world) {
            return Some(exit);
        }
        if !world.contains_resource::<MatchClock>() {
            return stop.then_some(AppExit::Success);
        }
        if world.resource::<Session>().log().result().is_none() {
            if !stop && !SimServer::players_gone(world) {
                return None;
            }
            let outcome = Session::outcome(world);
            if let Err(error) = SimServer::end_session(world, outcome) {
                world
                    .resource_mut::<Faults>()
                    .report(Fault::Snapshot(error));
                return Faults::apply(world);
            }
        }
        let session = world.resource::<Session>();
        let id = session.log().session_id();
        let hash = session.state_hash(world);
        let data = world.resource::<ServerData>();
        Some(match SessionDir::publish(data, session.log()) {
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
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::num::{NonZeroU8, NonZeroU32};

    use campfire_package::{ModePackages, PackageDir};
    use campfire_protocol::{SeedChain, SessionHeader, SessionLog, SlotPlan, SlotStart};
    use campfire_runner::{InputRules, SessionRules};
    use campfire_store::{AppendFile, AppendWriter};

    use super::*;
    use crate::journal_watch::JournalWatch;
    use crate::local_match;
    use crate::session_journal::SessionJournal;

    /// A journal's file whose every sync fails.
    #[derive(Debug)]
    struct FailingFile;

    impl AppendFile for FailingFile {
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
        let packages = ModePackages::from_dir(&PackageDir::workspace("test/modes/lane")).unwrap();
        let key = local_match::keypair(8);
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
        let journal = SessionJournal(AppendWriter::start("journal", FailingFile));
        let mut world = World::new();
        world.init_resource::<Faults>();
        world.insert_resource(JournalWatch(journal.watch()));
        assert_eq!(ServerExit::due(&mut world, false), None);
        log.keep_journal(Box::new(journal));
        // Dropped, the log's journal waits for its writer, which stopped at the failed sync.
        drop(log);
        let code = NonZeroU8::new(ServerExit::STORAGE_FAILED).unwrap();
        assert_eq!(
            ServerExit::due(&mut world, false),
            Some(AppExit::Error(code))
        );
        // The failure is given once; before the match, only a stop exits.
        assert_eq!(ServerExit::due(&mut world, false), None);
        assert_eq!(ServerExit::due(&mut world, true), Some(AppExit::Success));
    }
}
