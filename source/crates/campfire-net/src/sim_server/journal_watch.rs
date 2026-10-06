use bevy_ecs::resource::Resource;
use bevy_ecs::system::Res;
use campfire_log::LogEvent;
use campfire_store::AppendWatch;

use crate::events::journal_sync_slow::JournalSyncSlow;

/// The running session's journal, as the server watches it: how many records are durable, and
/// whether a write or a sync failed. Present once the match starts with a journal.
#[derive(Resource, Debug, Clone)]
pub struct JournalWatch(pub AppendWatch);

impl JournalWatch {
    /// Logs the slowest sync of the journal since the last frame, when one was slow.
    pub(crate) fn warn_slow(journal: Res<'_, JournalWatch>) {
        if let Some(slow) = journal.0.take_slow_sync() {
            JournalSyncSlow {
                took_ns: u64::try_from(slow.took.as_nanos()).unwrap_or(u64::MAX),
            }
            .log();
        }
    }
}
