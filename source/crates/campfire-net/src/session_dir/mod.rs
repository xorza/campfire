use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use campfire_package::{ModePackages, RELEASE};
use campfire_protocol::secp256k1::{Keypair, Secp256k1};
use campfire_protocol::{
    DurableError, DurableFile, Journal, JournalError, JournalFrames, Outcome, SessionLog,
    SessionPrivate,
};
use campfire_runner::Runner;

use crate::session_dir::error::{AbortError, FindError, RestoreError};

pub(crate) mod error;

/// A session's directory under a server's data directory, `sessions/<session id>`: its `private`
/// record, written once before the first offer, and its `journal`. Each is synced into its parent
/// as it is made, so a crash loses neither. Once the session ends, its log goes to the data
/// directory's `logs`, `<session id>.campfire-log`, which marks it done.
#[derive(Debug)]
pub struct SessionDir {
    path: PathBuf,
}

/// A session read back from its directory, as a server that starts again finds it.
#[derive(Debug)]
pub struct RestoredSession {
    pub private: SessionPrivate,
    /// The log its journal rebuilds: what the server held when it wrote the last record.
    pub log: SessionLog,
    /// Its journal, cut to its whole frames, open to append.
    pub journal: Journal,
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

    /// The session's new journal.
    pub fn start_journal(&self) -> Result<Journal, JournalError> {
        Journal::create(&self.path.join("journal"))
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
            journal,
            modified,
        }))
    }

    /// Removes the directory of a session whose match never started, which logged nothing.
    pub fn remove(self) -> io::Result<()> {
        fs::remove_dir_all(&self.path)
    }

    /// Publishes `log`, its seed revealed, as `logs/<session id>.campfire-log` under `data`,
    /// written durably; the file's path.
    pub fn publish(data: &Path, log: &SessionLog) -> Result<PathBuf, DurableError> {
        let logs = data.join("logs");
        DurableFile::create_dir(&logs)?;
        let file = logs.join(format!("{}.campfire-log", log.session_id()));
        let mut bytes = Vec::new();
        log.encode(&mut bytes);
        DurableFile::write(&file, &bytes)?;
        Ok(file)
    }
}

impl RestoredSession {
    /// Ends the session aborted, as a server back past the restore window does, unless it
    /// ended before the crash: replays the log, of the mode `packages` holds, to the state its
    /// last tick left; logs an aborted result there, which `server_key` signs with the auxiliary
    /// randomness `aux`, into the journal too; and publishes the log under `data`, its seed
    /// revealed. The published file's path.
    pub fn abort(
        self,
        data: &Path,
        packages: &ModePackages,
        server_key: &Keypair,
        aux: &[u8; 32],
    ) -> Result<PathBuf, AbortError> {
        let ticks = self.log.next_tick();
        let seeds = self.private.seed_chain.seeds();
        let mut runner =
            Runner::new(self.log.rewound(), seeds, packages).map_err(AbortError::Start)?;
        runner.resume_journal(self.journal);
        while runner.log().next_tick() < ticks {
            runner.run_tick();
        }
        if runner.log().result().is_none() {
            let result = runner.result_as(Outcome::Aborted);
            let id = runner.log().session_id();
            let signature = result.sign(&Secp256k1::new(), server_key, id, aux);
            runner
                .record_result(result, &signature)
                .expect("the server's own result holds");
        }
        runner.reveal_seed();
        SessionDir::publish(data, runner.log()).map_err(AbortError::Publish)
    }
}
