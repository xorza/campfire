use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use bevy_ecs::world::World;
use campfire_common::{ErrorReport, Version};
use campfire_log::LogEvent;
use campfire_package::ModePackages;
use campfire_protocol::{
    JournalFrames, PublishWriteError, ServerSeed, SessionId, SessionLog, SessionPrivate,
    SessionTerms, Signer,
};
use campfire_runner::{
    CheckedTerms, ResumeAt, Session, SessionRules, SessionSource, SessionWorld, StartError,
};
use campfire_store::{
    AppendOpenError, DirEntries, DurableDir, DurableDirError, DurableFile, InputFile, PathError,
    ReadError, Stamped,
};
use tracing::{error, info};

use crate::events::journal_cut::JournalCut;
use crate::events::session_aborted::SessionAborted;
use crate::server::checkpoints::Checkpoints;
use crate::server::error::MatchBeginError;
use crate::server::journal_watch::{JournalFrom, JournalWatch};
use crate::server::reports::{DueReport, Reports};
use crate::server::server_dir::ServerDir;
use crate::server::session_dir::error::{
    AbortError, FindError, RestoreError, SessionCreateError, SessionPublishError, WaitingError,
};
use crate::server::session_journal::SessionJournal;
use crate::server::snapshot_dir::SnapshotDir;

pub(crate) mod error;

/// A session's directory under a server's data directory, `sessions/<session id>`: its `private`
/// record, written once before the first offer, its `journal`, and its checkpoints' `snapshots`.
/// Each is synced into its parent as it is made, so a crash loses none. Once the session ends,
/// its log is published in the data directory, which marks it done.
#[derive(Debug)]
pub struct SessionDir {
    session: SessionId,
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
    /// The log its journal rebuilds, whose header holds the session's terms: what the server
    /// held when it wrote the last record.
    pub log: SessionLog,
    /// Its journal, cut to its whole frames, open to append, and its snapshots.
    pub files: SessionFiles,
    /// The bytes its journal holds: the tag and the whole frames.
    pub journal_len: u64,
    /// When the journal was last written.
    pub modified: SystemTime,
}

/// What publishes a session that ends unrestored: the server's data directory, the packages of
/// its mode, and the server's signer, which signs its result.
#[derive(Debug, Clone, Copy)]
pub struct Publisher<'a> {
    pub data: &'a ServerDir,
    pub packages: &'a ModePackages,
    pub signer: &'a Arc<Signer>,
}

impl SessionDir {
    /// Makes the directory of `session` under `data`, and writes `private` into it.
    pub fn create(
        data: &ServerDir,
        session: SessionId,
        private: &SessionPrivate,
    ) -> Result<Self, SessionCreateError> {
        DurableDir::create(&data.layout().sessions_dir())?;
        let path = data.layout().session_dir(session);
        DurableDir::create(&path)?;
        let dir = Self { session, path };
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
    pub fn find(data: &ServerDir) -> Result<Option<Self>, FindError> {
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
            found = Some(Self {
                session,
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
        let Some(dir) = Self::find(data).map_err(WaitingError::Find)? else {
            return Ok(None);
        };
        let session = dir.restore().map_err(WaitingError::Restore)?;
        if session.is_none() {
            dir.remove().map_err(WaitingError::Remove)?;
        }
        Ok(session)
    }

    /// Reads the session back: its private record, and the log its journal rebuilds, the
    /// journal cut at its first bad frame, which logs `JournalCut` with the bytes it drops
    /// (08-sessions.md, A journal is cut at its first bad frame); none when the journal holds no
    /// record, as a session whose match never started has none, and none when a stop as the
    /// session opened left no private record or no journal. An error for a record that does not
    /// read, a journal whose records do not rebuild a log, as one of another release does not,
    /// one of another session, and a seed chain the terms do not commit to.
    pub fn restore(&self) -> Result<Option<RestoredSession>, RestoreError> {
        let private = InputFile::read_if_present(&self.private(), SessionPrivate::MAX_FILE_LEN)
            .map_err(RestoreError::Read)?;
        let Some(private) = private else {
            return Ok(None);
        };
        let private = SessionPrivate::decode(&private).map_err(RestoreError::Private)?;
        let path = self.journal();
        let Stamped { bytes, modified } =
            match InputFile::read_stamped(&path, SessionLog::MAX_JOURNAL_LEN) {
                Ok(stamped) => stamped,
                Err(PathError {
                    error: ReadError::Missing,
                    ..
                }) => return Ok(None),
                Err(error) => return Err(RestoreError::Read(error)),
            };
        let mut frames = JournalFrames::new(&bytes).map_err(RestoreError::NotJournal)?;
        let records: Vec<&[u8]> = frames.by_ref().collect();
        let len = |bytes: usize| u64::try_from(bytes).expect("a file's length fits u64");
        let whole = len(frames.whole());
        let cut = JournalCut {
            session: self.session,
            bytes: len(bytes.len()) - whole,
        };
        if records.is_empty() {
            cut.log_dropped();
            return Ok(None);
        }
        let log =
            SessionLog::from_journal(records, Version::ENGINE).map_err(RestoreError::Replay)?;
        if log.session_id() != self.session {
            return Err(RestoreError::OtherSession(log.session_id()));
        }
        if private.seed_chain.commitment() != log.header().terms.seed_commitment {
            return Err(RestoreError::OtherSeedChain);
        }
        let journal = SessionJournal::reopen(&path, whole).map_err(RestoreError::Journal)?;
        cut.log_dropped();
        Ok(Some(RestoredSession {
            private,
            log,
            files: SessionFiles {
                journal,
                snapshots: self.snapshots(),
            },
            journal_len: whole,
            modified,
        }))
    }

    /// Removes the directory of a session whose match never started, which logged nothing.
    pub fn remove(self) -> Result<(), PathError<DurableDirError>> {
        DurableDir::remove(&self.path)
    }

    /// Publishes the log of `session`, which its journal under `data` holds whole, with the
    /// server seed `revealed` of its last segment, in `data`, written durably, as `publish_into`
    /// writes it; the file's path.
    pub fn publish(
        data: &ServerDir,
        session: SessionId,
        revealed: &ServerSeed,
    ) -> Result<PathBuf, SessionPublishError> {
        DurableDir::create(&data.layout().logs_dir())?;
        let file = data.layout().published_log(session);
        Self::publish_into(data, session, Bound::Whole, revealed, &file)?;
        Ok(file)
    }

    /// Publishes the log so far into each report's folder once the journal synced its records,
    /// the last segment's seed revealed, as `publish_into` writes it; logs each report written,
    /// and each that failed.
    pub fn publish_reports(world: &mut World) {
        let Some(durable) = world
            .get_resource::<JournalWatch>()
            .map(|watch| watch.0.durable())
        else {
            return;
        };
        if world
            .get_resource::<Reports>()
            .is_none_or(Reports::is_empty)
        {
            return;
        }
        let mut reports = world
            .remove_resource::<Reports>()
            .expect("the reports of a local server");
        let mut session = world.resource_mut::<Session>();
        let next = session.log().next_tick();
        if let Some(journal) = session.journal_mut() {
            reports.mark(next, journal);
        }
        let session = world.resource::<Session>();
        let (id, seed) = (session.log().session_id(), session.last_seed().clone());
        let data = world.resource::<ServerDir>();
        for DueReport { dir, bytes } in reports.due(id, durable) {
            let file = dir.log();
            if let Err(problem) = dir.create() {
                error!(file = %file.display(), error = %ErrorReport::of(&problem), "could not make the report's folder");
                continue;
            }
            match Self::publish_into(data, id, Bound::Bytes(bytes), &seed, &file) {
                Ok(()) => info!(file = %file.display(), "wrote the log so far into the report"),
                Err(problem) => {
                    error!(file = %file.display(), error = %ErrorReport::of(&problem), "could not write the log into the report");
                }
            }
        }
        world.insert_resource(reports);
    }

    /// Writes durably into `file` the log of `session`, whose journal under `data` holds it in
    /// `bound` of its bytes, with the server seed `revealed` of its last segment: the file the
    /// journal publishes in two streaming reads (08-sessions.md, A published log is written from
    /// the journal), so a session's log is never whole in memory, and a bound keeps a read off the
    /// frames the journal's writer appends after it.
    pub(crate) fn publish_into(
        data: &ServerDir,
        session: SessionId,
        bound: Bound,
        revealed: &ServerSeed,
        file: &Path,
    ) -> Result<(), SessionPublishError> {
        let dir = Self {
            session,
            path: data.layout().session_dir(session),
        };
        let journal = dir.journal();
        let mut failed = None;
        let mut walk = |visit: &mut dyn FnMut(&[u8])| {
            if failed.is_some() {
                return;
            }
            let streamed = InputFile::stream(&journal)
                .map_err(SessionPublishError::Read)
                .and_then(|stream| {
                    let streamed = match bound {
                        Bound::Whole => JournalFrames::stream(stream, visit),
                        Bound::Bytes(len) => JournalFrames::stream(stream.take(len), visit),
                    };
                    streamed.map_err(SessionPublishError::Journal)
                });
            if let Err(error) = streamed {
                failed = Some(error);
            }
        };
        // A journal that does not publish ends the write, and its error is the publication's.
        let mut unpublished = None;
        let mut fill = |out: &mut dyn Write| match SessionLog::publish(&mut walk, revealed, out) {
            Ok(()) => Ok(()),
            Err(PublishWriteError::Write(error)) => Err(error),
            Err(PublishWriteError::Journal(error)) => {
                unpublished = Some(error);
                Err(io::Error::other("the journal does not publish"))
            }
        };
        let written = DurableFile::write_with(file, &mut fill);
        if let Some(error) = failed {
            return Err(error);
        }
        if let Some(error) = unpublished {
            return Err(error.into());
        }
        written?;
        Ok(())
    }
}

/// How much of a session's journal a publication reads: all of it, as at the session's end, or
/// its first bytes, of the records durable when a report asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bound {
    Whole,
    Bytes(u64),
}

/// What a session's replay leaves beside its match: the watch of its journal, and its
/// snapshots' directory.
#[derive(Debug)]
pub struct Replayed {
    pub journal: JournalWatch,
    pub snapshots: SnapshotDir,
}

/// What a session's replay takes beside its world: the packages of its mode, the server's
/// signer, which signs what the session logs, and the log's terms as the mode's rules checked
/// them.
#[derive(Debug)]
pub struct ReplayWith<'p> {
    pub packages: &'p ModePackages,
    pub signer: Arc<Signer>,
    pub checked: CheckedTerms,
}

impl RestoredSession {
    /// The session's terms, which its journal's header holds.
    pub const fn terms(&self) -> &SessionTerms {
        &self.log.header().terms
    }

    /// Builds the session's match in `world`, of the mode `with.packages` holds: from the
    /// snapshot of the latest checkpoint whose record the log holds, or from tick 0 with none;
    /// resumes its journal; and replays the log from there to the tick the server stopped before,
    /// running `after_tick` after each tick the replay runs, and taking again a checkpoint begun
    /// with no record as the replay reaches its boundary; the session signs with `with.signer`,
    /// the server's. The watch of its journal, and its snapshots, which `world` holds neither of.
    pub fn replay(
        self,
        world: &mut World,
        with: ReplayWith<'_>,
        mut after_tick: impl FnMut(&mut World),
    ) -> Result<Replayed, MatchBeginError> {
        let ReplayWith {
            packages,
            signer,
            checked,
        } = with;
        let Self {
            private,
            log,
            files: SessionFiles { journal, snapshots },
            journal_len,
            ..
        } = self;
        let ticks = log.next_tick();
        let seeds = private.seed_chain.seeds();
        let latest = log
            .checkpoints()
            .last()
            .map(|record| (record.segment, record.snapshot));
        if let Some((segment, fingerprint)) = latest {
            let snapshot = snapshots
                .read(fingerprint)
                .map_err(MatchBeginError::ReadSnapshot)?;
            let source = SessionSource {
                log,
                seeds,
                packages,
                checked,
                signer: Some(signer),
            };
            let at = ResumeAt {
                segment,
                snapshot: &snapshot,
            };
            Session::resume(world, source, at).map_err(MatchBeginError::Resume)?;
        } else {
            let source = SessionSource {
                log,
                seeds,
                packages,
                checked,
                signer: Some(signer),
            };
            Session::start(world, source).map_err(MatchBeginError::Start)?;
        }
        let journal = JournalWatch::keep(
            &mut world.resource_mut::<Session>(),
            journal,
            JournalFrom::End { len: journal_len },
        );
        let resumed = world.session().next_tick();
        world.replay_to(ticks, |world| {
            if world.session().next_tick() > resumed {
                after_tick(world);
            }
            Checkpoints::take_again(world, &snapshots).map_err(MatchBeginError::WriteSnapshot)
        })?;
        Ok(Replayed { journal, snapshots })
    }

    /// Ends the session, as a server back past the restore window does, unless it ended before the
    /// crash: replays the log, of the mode `publisher` names, as `replay` does, to the state its
    /// last tick left; logs the result there, aborted unless the mode ended the match, into the
    /// journal too; and publishes the log under the publisher's data directory, its seed revealed,
    /// and logs `SessionAborted`. The publisher's key signs what it logs. The published file's
    /// path.
    pub fn abort(self, publisher: Publisher<'_>) -> Result<PathBuf, AbortError> {
        let Publisher {
            data,
            packages,
            signer,
        } = publisher;
        let checked = SessionRules::of(packages)
            .check(self.terms())
            .map_err(|error| {
                AbortError::Replay(MatchBeginError::Start(StartError::Terms(error)))
            })?;
        let with = ReplayWith {
            packages,
            signer: Arc::clone(signer),
            checked,
        };
        let mut world = World::new();
        self.replay(&mut world, with, |_| {})
            .map_err(AbortError::Replay)?;
        if world.session().log().result().is_none() {
            let result = world.session_result();
            world
                .session_mut()
                .record_result(result)
                .expect("the server's own result holds");
        }
        world.session_mut().reveal_seed();
        world.session_mut().close_journal();
        let log = world.resource::<Session>().log();
        let revealed = log.revealed_seeds().expect("the seed is revealed");
        let last = revealed
            .seed(log.segment())
            .expect("the seeds reach the last segment");
        let file =
            SessionDir::publish(data, log.session_id(), last).map_err(AbortError::Publish)?;
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

    use campfire_common::{MapName, PackageFingerprint, Ticks};
    use campfire_protocol::internals::TestKey;
    use campfire_protocol::{InputRules, PlayerSlotPlan, SeedChain};
    use campfire_store::internals::TestDir;
    use campfire_visibility::Visibility;

    use super::*;

    #[test]
    fn a_session_stopped_at_any_step_before_its_first_record_goes() {
        let test_dir = TestDir::new();
        let root = test_dir.path("data");
        let data = ServerDir::open(&root).unwrap();
        let key = TestKey::of(8);
        let seed_chain = SeedChain::new([9; 32], NonZeroU32::MIN);
        let terms = SessionTerms {
            server_key: key.x_only_public_key().0,
            tick_hz: NonZeroU32::new(30).unwrap(),
            inputs: InputRules {
                max_lead: Ticks::new(3),
                max_payload_len: 64,
                max_per_tick: 4,
            },
            seed_commitment: seed_chain.commitment(),
            release: Version::ENGINE,
            mode: PackageFingerprint::new([1; 32]),
            map: MapName::parse("lane").unwrap(),
            dependencies: Vec::new(),
            player_slots: vec![PlayerSlotPlan::Open],
            visibility: Visibility::DEFAULT.encode(),
            options: Vec::new(),
            player_slot_picks: Vec::new(),
        };
        let private = SessionPrivate { seed_chain };
        let id = terms.session_id();
        let session = Path::new("data/sessions").join(id.to_string());
        let path = test_dir.path(&session);
        let gone = |data: &ServerDir| {
            assert!(SessionDir::waiting(data).unwrap().is_none());
            !test_dir.exists(&session)
        };
        // Stopped with its directory made, before its private record.
        DurableDir::create(&root.join("sessions")).unwrap();
        DurableDir::create(&path).unwrap();
        assert!(gone(&data));
        // Before its journal.
        SessionDir::create(&data, id, &private).unwrap();
        assert!(gone(&data));
        // With its journal's tag and no record.
        let dir = SessionDir::create(&data, id, &private).unwrap();
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
