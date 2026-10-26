use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_protocol::{Journal, SessionId, SessionLog};
use campfire_runner::Session;
use campfire_store::DurableFile;
use tracing::warn;

use crate::client::client_layout::ClientLayout;
use crate::client::report_dir::ReportDir;
use crate::error::{JournalRefusal, ReportWriteError};
use crate::messages::journal_command::ReportKey;

/// A local server's reports of its log, each a player asked of the tick its client drew: the
/// data directory of the client whose reports they are, the ticks the log has yet to reach, and
/// the reports whose records the journal has yet to sync (08-sessions.md, The log is the local
/// server's, published at the report's tick).
#[derive(Resource, Debug)]
pub struct Reports {
    layout: ClientLayout,
    asked: Vec<ReportKey>,
    pending: Vec<PendingReport>,
}

/// A report whose tick the log reached: the report, the records the journal had taken, and the
/// bytes of the journal's file they end at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingReport {
    key: ReportKey,
    records: u64,
    bytes: u64,
}

/// A report whose records the journal synced: its folder, and the bytes of the journal's file
/// its log ends at.
#[derive(Debug)]
pub(crate) struct DueReport {
    pub(crate) dir: ReportDir,
    pub(crate) bytes: u64,
}

impl Reports {
    /// The reports of the client of the data directory `layout`, none asked yet.
    pub const fn new(layout: ClientLayout) -> Self {
        Self {
            layout,
            asked: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// Whether no report waits for the log or for the journal.
    pub(crate) const fn is_empty(&self) -> bool {
        self.asked.is_empty() && self.pending.is_empty()
    }

    /// Writes `log`, the whole log a replay replays, into the folder of the report `key` of its
    /// session, as a report of a replay holds the log that replays it (08-sessions.md, The client
    /// opens a report).
    pub fn write_replayed(&self, key: ReportKey, log: &SessionLog) -> Result<(), ReportWriteError> {
        let dir = self.layout.report_dir(log.session_id(), key);
        dir.create()?;
        let mut bytes = Vec::new();
        log.encode(&mut bytes);
        Ok(DurableFile::write(&dir.log(), &bytes)?)
    }

    /// Takes the report `key`, of the last tick its client ran, which a predicting client runs
    /// before the server; why not, on a server of no reports or of no journal.
    pub(crate) fn take(world: &mut World, key: ReportKey) -> Result<(), JournalRefusal> {
        if !world.contains_resource::<Self>() {
            return Err(JournalRefusal::NoData);
        }
        if world.resource::<Session>().journal().is_none() {
            return Err(JournalRefusal::NoData);
        }
        world.resource_mut::<Self>().asked.push(key);
        Ok(())
    }

    /// Drops each report whose tick the log has yet to reach, as a load went back to a save and
    /// the log will reach that tick in another history than the one the client drew; logs each.
    pub(crate) fn loaded(world: &mut World) {
        let Some(mut reports) = world.get_resource_mut::<Self>() else {
            return;
        };
        for key in reports.asked.drain(..) {
            warn!(tick = %key.tick, take = key.take, "a load dropped a report the log had yet to reach");
        }
    }

    /// Marks each report whose tick the log ran, before its next tick `next`, as pending on the
    /// records `journal` took, which it hands its writer.
    pub(crate) fn mark(&mut self, next: Tick, journal: &mut Journal) {
        if !self.asked.iter().any(|key| key.tick < next) {
            return;
        }
        journal.flush();
        let (records, bytes) = (journal.written(), journal.file_len());
        let pending = &mut self.pending;
        self.asked.retain(|&key| {
            let reached = key.tick < next;
            if reached {
                pending.push(PendingReport {
                    key,
                    records,
                    bytes,
                });
            }
            !reached
        });
    }

    /// Takes each pending report of the session `session` whose records the journal synced,
    /// `durable` of them.
    pub(crate) fn due(
        &mut self,
        session: SessionId,
        durable: u64,
    ) -> impl Iterator<Item = DueReport> {
        let layout = &self.layout;
        self.pending
            .extract_if(.., move |report| report.records <= durable)
            .map(move |report| DueReport {
                dir: layout.report_dir(session, report.key),
                bytes: report.bytes,
            })
    }
}

/// A report a stopped server did not write its log into, as the session ended before the log
/// reached its tick or the journal synced it, is logged.
impl Drop for Reports {
    fn drop(&mut self) {
        let asked = self.asked.iter();
        for key in asked.chain(self.pending.iter().map(|report| &report.key)) {
            warn!(tick = %key.tick, take = key.take, "the server stopped before it wrote a report's log");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_load_drops_the_reports_the_log_has_yet_to_reach_and_keeps_the_rest() {
        let mut world = World::new();
        let mut reports = Reports::new(ClientLayout::at(PathBuf::from("client")));
        let key = |tick| ReportKey {
            tick: Tick::new(tick),
            take: 0,
        };
        reports.asked.push(key(40));
        let reached = PendingReport {
            key: key(20),
            records: 7,
            bytes: 900,
        };
        reports.pending.push(reached);
        world.insert_resource(reports);
        Reports::loaded(&mut world);
        let reports = world.resource::<Reports>();
        assert_eq!(reports.asked, []);
        assert_eq!(reports.pending, [reached]);
        // A server of no reports loads too.
        Reports::loaded(&mut World::new());
    }
}
