use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy_ecs::world::{Mut, World};
use campfire_log::LogEvent;
use campfire_package::{ModePackages, RELEASE};
use campfire_protocol::secp256k1::Keypair;
use campfire_protocol::{
    DurableError, DurableFile, Journal, JournalError, JournalFrames, Outcome, SessionId,
    SessionLog, SessionPrivate,
};
use campfire_runner::Session;

use crate::checkpoints::Checkpoints;
use crate::events::session_aborted::SessionAborted;
use crate::server_signer::ServerSigner;
use crate::session_dir::error::{AbortError, FindError, RestoreError, WaitingError};

pub(crate) mod error;

/// A session's directory under a server's data directory, `sessions/<session id>`: its `private`
/// record, written once before the first offer, its `journal`, and its checkpoints' `snapshots`.
/// Each is synced into its parent as it is made, so a crash loses none. Once the session ends,
/// its log goes to the data directory's `logs`, `<session id>.campfire-log`, which marks it
/// done.
#[derive(Debug)]
pub struct SessionDir {
    path: PathBuf,
}

/// What a session writes as it runs: its journal, and the directory its checkpoints' snapshots
/// go to, each named by its fingerprint in hex.
#[derive(Debug)]
pub struct SessionFiles {
    pub journal: Journal,
    pub snapshots: PathBuf,
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
    pub fn create(data: &Path, private: &SessionPrivate) -> Result<SessionDir, DurableError> {
        let sessions = data.join("sessions");
        DurableFile::create_dir(&sessions)?;
        let path = sessions.join(private.terms.session_id().to_string());
        DurableFile::create_dir(&path)?;
        DurableFile::write(&path.join("private"), &private.encode())?;
        Ok(SessionDir { path })
    }

    /// The session's files: its new journal, and where its snapshots go.
    pub fn start(&self) -> Result<SessionFiles, JournalError> {
        Ok(SessionFiles {
            journal: Journal::create(&self.path.join("journal"))?,
            snapshots: self.snapshots(),
        })
    }

    fn snapshots(&self) -> PathBuf {
        self.path.join("snapshots")
    }

    /// The directory of the one session under `data` whose log is not published; none when
    /// every session's is. An error when there are several.
    pub fn find(data: &Path) -> Result<Option<SessionDir>, FindError> {
        let entries = match fs::read_dir(data.join("sessions")) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(FindError::Read(error)),
        };
        let mut found = None;
        for entry in entries {
            let entry = entry.map_err(FindError::Read)?;
            let mut published = entry.file_name();
            published.push(".campfire-log");
            let done = data
                .join("logs")
                .join(published)
                .try_exists()
                .map_err(FindError::Read)?;
            if done {
                continue;
            }
            if found.is_some() {
                return Err(FindError::Several);
            }
            found = Some(SessionDir { path: entry.path() });
        }
        Ok(found)
    }

    /// The session a stop left under `data`, read back, as a server that starts again takes it
    /// up: none when every session's log is published, and none when its match never started,
    /// whose directory goes. An error as `find` and `restore` give one, and for a directory that
    /// does not go.
    pub fn waiting(data: &Path) -> Result<Option<RestoredSession>, WaitingError> {
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
    /// whose match never started has none. An error for a record that does not read, a session
    /// of another release, and a journal whose records do not rebuild a log.
    pub fn restore(&self) -> Result<Option<RestoredSession>, RestoreError> {
        let private = fs::read(self.path.join("private")).map_err(RestoreError::Read)?;
        let private = SessionPrivate::decode(&private).map_err(RestoreError::Private)?;
        if private.terms.release != RELEASE {
            return Err(RestoreError::OtherRelease(private.terms.release));
        }
        let path = self.path.join("journal");
        let modified = fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .map_err(RestoreError::Read)?;
        let bytes = fs::read(&path).map_err(RestoreError::Read)?;
        let mut frames = JournalFrames::new(&bytes).map_err(RestoreError::Journal)?;
        let records: Vec<&[u8]> = frames.by_ref().collect();
        if records.is_empty() {
            return Ok(None);
        }
        let log = SessionLog::from_journal(records).map_err(RestoreError::Replay)?;
        let whole = u64::try_from(frames.whole()).expect("a file's length fits u64");
        let journal = Journal::reopen(&path, whole).map_err(RestoreError::Journal)?;
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

    /// Where the log of the session `session` is published under `data`.
    pub fn published(data: &Path, session: SessionId) -> PathBuf {
        data.join("logs").join(format!("{session}.campfire-log"))
    }

    /// Removes the directory of a session whose match never started, which logged nothing.
    pub fn remove(self) -> io::Result<()> {
        fs::remove_dir_all(&self.path)
    }

    /// Publishes `log`, its seed revealed, as `logs/<session id>.campfire-log` under `data`,
    /// written durably; the file's path.
    pub fn publish(data: &Path, log: &SessionLog) -> Result<PathBuf, DurableError> {
        DurableFile::create_dir(&data.join("logs"))?;
        let file = SessionDir::published(data, log.session_id());
        let mut bytes = Vec::new();
        log.encode(&mut bytes);
        DurableFile::write(&file, &bytes)?;
        Ok(file)
    }
}

impl RestoredSession {
    /// Ends the session aborted, as a server back past the restore window does, unless it
    /// ended before the crash: replays the log, of the mode `packages` holds, to the state its
    /// last tick left, taking again a checkpoint begun with no record as it reaches its
    /// boundary; logs an aborted result there, into the journal too; and publishes the log under
    /// `data`, its seed revealed, and logs `SessionAborted`. `server_key` signs what it logs, with
    /// auxiliary randomness from `entropy`. The published file's path.
    pub fn abort(
        self,
        data: &Path,
        packages: &ModePackages,
        server_key: Keypair,
        entropy: fn(&mut [u8; 32]),
    ) -> Result<PathBuf, AbortError> {
        let ticks = self.log.next_tick();
        let seeds = self.private.seed_chain.seeds();
        let mut world = World::new();
        Session::start(&mut world, self.log.rewound(), seeds, packages)
            .map_err(AbortError::Start)?;
        world
            .resource_mut::<Session>()
            .resume_journal(self.files.journal);
        let signer = ServerSigner::new(server_key, entropy);
        loop {
            Checkpoints::take_again(&mut world, &self.files.snapshots, &signer)
                .map_err(AbortError::Snapshot)?;
            if world.resource::<Session>().log().next_tick() >= ticks {
                break;
            }
            Session::run_tick(&mut world);
        }
        world.resource_scope(|world, mut session: Mut<'_, Session>| {
            if session.log().result().is_none() {
                let result = session.result(world, Outcome::Aborted);
                let signature = signer.sign_result(&result, session.log().session_id());
                session
                    .record_result(result, &signature)
                    .expect("the server's own result holds");
            }
            session.reveal_seed();
        });
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
