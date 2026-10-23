use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Res, ResMut};
use bevy_ecs::world::World;
use campfire_log::LogEvent;
use campfire_runner::Session;
use campfire_store::AppendWatch;

use crate::events::journal_sync_slow::JournalSyncSlow;
use crate::faults::WorkerFaults;
use crate::faults::fault::Fault;
use crate::server::session_journal::SessionJournal;

/// The running session's journal, as the server watches it: how many records are durable, and
/// whether a write or a sync failed. Present once the match starts with a journal.
#[derive(Resource, Debug, Clone)]
pub struct JournalWatch(pub AppendWatch);

/// Where a journal the session keeps goes on from.
#[derive(Debug, Clone, Copy)]
pub enum JournalFrom {
    /// A new journal, which the log fills from its start.
    Start,
    /// A journal of `len` bytes, which holds every record of the log so far.
    End { len: u64 },
}

impl JournalWatch {
    /// Keeps `journal` as the journal of `session`, from `from`; the watch of it.
    pub fn keep(session: &mut Session, journal: SessionJournal, from: JournalFrom) -> Self {
        let watch = Self(journal.watch());
        let journal = Box::new(journal);
        match from {
            JournalFrom::Start => session.keep_journal(journal),
            JournalFrom::End { len } => session.resume_journal(journal, len),
        }
        watch
    }

    /// Reports to `WorkerFaults` the journal's failure, when a write or a sync failed since the last
    /// frame.
    pub fn report_failure(world: &mut World) {
        let failure = world
            .get_resource::<Self>()
            .and_then(|journal| journal.0.take_failure());
        if let Some(error) = failure {
            world
                .resource_mut::<WorkerFaults>()
                .report(Fault::Journal(error));
        }
    }

    /// Hands the journal's sink the records the session's log took this frame, as one append,
    /// so its writer wakes once for each frame (08-sessions.md, A record is encoded once).
    pub fn flush(mut session: ResMut<'_, Session>) {
        if let Some(journal) = session.journal_mut() {
            journal.flush();
        }
    }

    /// Logs the slowest sync of the journal since the last frame, when one was slow.
    pub fn warn_slow(journal: Res<'_, Self>) {
        if let Some(slow) = journal.0.take_slow_sync() {
            JournalSyncSlow {
                took_ns: u64::try_from(slow.took.as_nanos()).unwrap_or(u64::MAX),
            }
            .log();
        }
    }
}
