use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy_ecs::world::World;
use campfire_log::LogEvent;
use campfire_package::ModePackages;
use campfire_protocol::{JournalFrames, Outcome, SessionId, SessionLog, SessionPrivate};
use campfire_runner::{Session, SessionWorld};
use campfire_sim::StateRegistry;
use campfire_store::{
    AppendOpenError, DirEntries, DurableDir, DurableError, DurableFile, InputFile, PathError,
    ReadError, Stamped,
};
use secp256k1::Keypair;

use crate::events::session_aborted::SessionAborted;
use crate::os::Entropy;
use crate::sim_server::checkpoints::Checkpoints;
use crate::sim_server::error::RestoreMatchError;
use crate::sim_server::journal_watch::JournalWatch;
use crate::sim_server::server_dir::ServerDir;
use crate::sim_server::server_signer::ServerSigner;
use crate::sim_server::session_dir::error::{AbortError, FindError, RestoreError, WaitingError};
use crate::sim_server::session_journal::SessionJournal;
use crate::sim_server::snapshot_dir::SnapshotDir;

pub(crate) mod error;

/// A session's directory under a server's data directory, `sessions/<session id>`: its `private`
/// record, written once before the first offer, its `journal`, and its checkpoints' `snapshots`.
/// Each is synced into its parent as it is made, so a crash loses none. Once the session ends,
/// its log is published in the data directory, which marks it done.
#[derive(Debug)]
pub struct SessionDir {
    path: PathBuf,
}

/// What a session writes as it runs: its journal, and the directory its checkpoints' snapshots
/// go to.
#[derive(Debug)]
pub struct SessionFiles {
    pub journal: SessionJournal,
    pub snapshots: SnapshotDir,
}

/// A session read back from its directory, as a server that starts again finds it.
#[derive(Debug)]
pub struct RestoredSession {
    pub private: SessionPrivate,
    /// The log its journal rebuilds: what the server held when it wrote the last record.
    pub log: SessionLog,
    /// Its journal, cut to its whole frames, open to append, and its snapshots.
    pub files: SessionFiles,
    /// When the journal was last written.
    pub modified: SystemTime,
}

impl SessionDir {
    /// Makes the directory of the session whose terms `private` holds, under `data`, and writes
    /// `private` into it.
    pub fn create(
        data: &ServerDir,
        private: &SessionPrivate,
    ) -> Result<SessionDir, PathError<DurableError>> {
        DurableDir::create(&data.layout().sessions_dir())?;
        let path = data.layout().session_dir(private.terms.session_id());
        DurableDir::create(&path)?;
        let dir = SessionDir { path };
        let mut bytes = Vec::new();
        private.encode(&mut bytes);
        DurableFile::write(&dir.private(), &bytes)?;
        Ok(dir)
    }

    /// The session's files: its new journal, and where its snapshots go.
    pub fn start(&self) -> Result<SessionFiles, PathError<AppendOpenError>> {
        Ok(SessionFiles {
            journal: SessionJournal::create(&self.journal())?,
            snapshots: self.snapshots(),
        })
    }

    fn private(&self) -> PathBuf {
        self.path.join("private")
    }

    fn journal(&self) -> PathBuf {
        self.path.join("journal")
    }

    fn snapshots(&self) -> SnapshotDir {
        SnapshotDir(self.path.join("snapshots"))
    }

    /// The directory of the one session under `data` whose log is not published; none when
    /// every session's is. An error when there are several, and for an entry named by no
    /// session id. The sessions and the published logs are each listed once, so every answer
    /// comes from one moment, and in the order of their names, so a directory with several
    /// flaws gives the same error on every OS.
    pub fn find(data: &ServerDir) -> Result<Option<SessionDir>, FindError> {
        let layout = data.layout();
        let listed = |path: &Path| match DirEntries::read(path) {
            Ok(entries) => Ok(entries),
            Err(PathError {
                error: ReadError::Missing,
                ..
            }) => Ok(Vec::new()),
            Err(error) => Err(FindError::Read(error)),
        };
        let published: BTreeSet<OsString> = listed(&layout.logs_dir())?
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        let mut found = None;
        for entry in listed(&layout.sessions_dir())? {
            let session: SessionId = entry
                .name
                .to_str()
                .and_then(|name| name.parse().ok())
                .ok_or(FindError::Stray(entry.name))?;
            let log = layout.published_log(session);
            let log = log.file_name().expect("a published log names a file");
            if published.contains(log) {
                continue;
            }
            if found.is_some() {
                return Err(FindError::Several);
            }
            found = Some(SessionDir {
                path: layout.session_dir(session),
            });
        }
        Ok(found)
    }

    /// The session a stop left under `data`, read back, as a server that starts again takes it
    /// up: none when every session's log is published, and none when its match never started,
    /// whose directory goes. An error as `find` and `restore` give one, and for a directory that
    /// does not go.
    pub fn waiting(data: &ServerDir) -> Result<Option<RestoredSession>, WaitingError> {
        let Some(dir) = SessionDir::find(data).map_err(WaitingError::Find)? else {
            return Ok(None);
        };
        let session = dir.restore().map_err(WaitingError::Restore)?;
        if session.is_none() {
            dir.remove().map_err(WaitingError::Remove)?;
        }
        Ok(session)
    }

    /// Reads the session back: its private record, and the log its journal rebuilds, the
    /// journal cut to its whole frames; none when the journal holds no record, as a session
    /// whose match never started has none, and none when a stop as the session opened left no
    /// private record or no journal. An error for a record that does not read, a session of
    /// another release, and a journal whose records do not rebuild a log.
    pub fn restore(&self) -> Result<Option<RestoredSession>, RestoreError> {
        let private = InputFile::read_if_present(&self.private(), SessionPrivate::MAX_FILE_LEN)
            .map_err(RestoreError::Read)?;
        let Some(private) = private else {
            return Ok(None);
        };
        let private = SessionPrivate::decode(&private).map_err(RestoreError::Private)?;
        if !private.terms.release.is_current() {
            return Err(RestoreError::OtherRelease(private.terms.release));
        }
        let path = self.journal();
        let Stamped { bytes, modified } =
            match InputFile::read_stamped(&path, SessionLog::MAX_FILE_LEN) {
                Ok(stamped) => stamped,
                Err(PathError {
                    error: ReadError::Missing,
                    ..
                }) => return Ok(None),
                Err(error) => return Err(RestoreError::Read(error)),
            };
        let mut frames = JournalFrames::new(&bytes).map_err(RestoreError::NotJournal)?;
        let records: Vec<&[u8]> = frames.by_ref().collect();
        if records.is_empty() {
            return Ok(None);
        }
        let log = SessionLog::from_journal(records).map_err(RestoreError::Replay)?;
        let whole = u64::try_from(frames.whole()).expect("a file's length fits u64");
        let journal = SessionJournal::reopen(&path, whole).map_err(RestoreError::Journal)?;
        Ok(Some(RestoredSession {
            private,
            log,
            files: SessionFiles {
                journal,
                snapshots: self.snapshots(),
            },
            modified,
        }))
    }

    /// Removes the directory of a session whose match never started, which logged nothing.
    pub fn remove(self) -> Result<(), PathError<DurableError>> {
        Ok(DurableDir::remove(&self.path)?)
    }

    /// Publishes `log`, its seed revealed, in `data`, written durably; the file's path.
    pub fn publish(data: &ServerDir, log: &SessionLog) -> Result<PathBuf, PathError<DurableError>> {
        DurableDir::create(&data.layout().logs_dir())?;
        let file = data.layout().published_log(log.session_id());
        let mut bytes = Vec::new();
        log.encode(&mut bytes);
        DurableFile::write(&file, &bytes)?;
        Ok(file)
    }
}

impl RestoredSession {
    /// Builds the session's match in `world`, of the mode `packages` holds: from the snapshot of
    /// the latest checkpoint whose record the log holds, or from tick 0 with none; resumes its
    /// journal, which `JournalWatch` watches; and replays the log from there to the tick the
    /// server stopped before, running `after_tick` after each tick the replay runs, and taking
    /// again, signed by `signer`, a checkpoint begun with no record as the replay reaches its
    /// boundary. Its snapshots.
    pub(crate) fn replay(
        self,
        world: &mut World,
        packages: &ModePackages,
        signer: &ServerSigner,
        mut after_tick: impl FnMut(&mut World),
    ) -> Result<SnapshotDir, RestoreMatchError> {
        let RestoredSession {
            private,
            log,
            files: SessionFiles { journal, snapshots },
            ..
        } = self;
        let ticks = log.next_tick();
        let seeds = private.seed_chain.seeds();
        let latest = log
            .checkpoints()
            .last()
            .map(|record| (record.segment, record.snapshot));
        if let Some((segment, fingerprint)) = latest {
            let snapshot = InputFile::read(
                &snapshots.file(fingerprint),
                StateRegistry::MAX_SNAPSHOT_LEN,
            )
            .map_err(RestoreMatchError::ReadSnapshot)?;
            Session::resume(world, log.rewound(), seeds, packages, segment, &snapshot)
                .map_err(RestoreMatchError::Resume)?;
        } else {
            Session::start(world, log.rewound(), seeds, packages)
                .map_err(RestoreMatchError::Start)?;
        }
        world.insert_resource(JournalWatch(journal.watch()));
        world.session_mut().resume_journal(Box::new(journal));
        let resumed = world.session().log().next_tick();
        world.replay_to(ticks, |world| {
            if world.session().log().next_tick() > resumed {
                after_tick(world);
            }
            Checkpoints::take_again(world, &snapshots, signer)
                .map_err(RestoreMatchError::WriteSnapshot)
        })?;
        Ok(snapshots)
    }

    /// Ends the session aborted, as a server back past the restore window does, unless it
    /// ended before the crash: replays the log, of the mode `packages` holds, as `replay` does,
    /// to the state its last tick left; logs an aborted result there, into the journal too; and
    /// publishes the log under
    /// `data`, its seed revealed, and logs `SessionAborted`. `server_key` signs what it logs, with
    /// auxiliary randomness from `entropy`. The published file's path.
    pub fn abort(
        self,
        data: &ServerDir,
        packages: &ModePackages,
        server_key: Keypair,
        entropy: Entropy,
    ) -> Result<PathBuf, AbortError> {
        let mut world = World::new();
        let signer = ServerSigner::new(server_key, entropy);
        self.replay(&mut world, packages, &signer, |_| {})
            .map_err(AbortError::Replay)?;
        if world.session().log().result().is_none() {
            let result = world.session_result_as(Outcome::Aborted);
            let mut session = world.session_mut();
            let signature = signer.sign_result(&result, session.log().session_id());
            session
                .record_result(result, &signature)
                .expect("the server's own result holds");
        }
        world.session_mut().reveal_seed();
        let log = world.resource::<Session>().log();
        let file = SessionDir::publish(data, log).map_err(AbortError::Publish)?;
        SessionAborted {
            session: log.session_id(),
            file: file.clone(),
        }
        .log();
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use campfire_common::{Fingerprint, MapName, Ticks};
    use campfire_protocol::internals::TestKey;
    use campfire_protocol::{EngineRelease, InputRules, SeedChain, SessionTerms, SlotPlan};
    use campfire_store::internals::Scratch;
    use campfire_visibility::Visibility;

    use super::*;

    #[test]
    fn a_session_stopped_at_any_step_before_its_first_record_goes() {
        let scratch = Scratch::new();
        let root = scratch.path("data");
        let data = ServerDir::open(&root).unwrap();
        let key = TestKey::of(8);
        let seed_chain = SeedChain::new([9; 32], NonZeroU32::MIN);
        let private = SessionPrivate {
            seed_chain,
            terms: SessionTerms {
                server_key: key.x_only_public_key().0,
                tick_hz: NonZeroU32::new(30).unwrap(),
                inputs: InputRules {
                    max_delay: Ticks::new(3),
                    max_lead: Ticks::new(3),
                    max_payload_len: 64,
                    max_per_tick: 4,
                },
                seed_commitment: seed_chain.commitment(),
                release: EngineRelease::CURRENT,
                mode: Fingerprint::new([1; 32]),
                map: MapName::new("lane").unwrap(),
                dependencies: Vec::new(),
                slots: vec![SlotPlan::Open],
                visibility: Visibility::DEFAULT.encode(),
            },
        };
        let session = Path::new("data/sessions").join(private.terms.session_id().to_string());
        let path = scratch.path(&session);
        let gone = |data: &ServerDir| {
            assert!(SessionDir::waiting(data).unwrap().is_none());
            !scratch.exists(&session)
        };
        // Stopped with its directory made, before its private record.
        DurableDir::create(&root.join("sessions")).unwrap();
        DurableDir::create(&path).unwrap();
        assert!(gone(&data));
        // Before its journal.
        SessionDir::create(&data, &private).unwrap();
        assert!(gone(&data));
        // With its journal's tag and no record.
        let dir = SessionDir::create(&data, &private).unwrap();
        drop(dir.start().unwrap());
        assert!(gone(&data));
        // An entry named by no session id is no session's.
        DurableDir::create(&root.join("sessions").join("stray")).unwrap();
        assert!(matches!(
            SessionDir::find(&data),
            Err(FindError::Stray(name)) if name == "stray"
        ));
        // Of several flaws, the first by name is the one found, whatever order the OS lists them
        // in: two unpublished sessions, whose ids sort before `stray`, then a stray entry whose
        // `-` sorts before them all.
        for id in ['0', '1'] {
            let name = String::from(id).repeat(64);
            DurableDir::create(&root.join("sessions").join(name)).unwrap();
        }
        assert!(matches!(SessionDir::find(&data), Err(FindError::Several)));
        DurableDir::create(&root.join("sessions").join("-stray")).unwrap();
        assert!(matches!(
            SessionDir::find(&data),
            Err(FindError::Stray(name)) if name == "-stray"
        ));
        drop(data);
    }
}
