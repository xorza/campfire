use bevy_app::AppExit;
use bevy_ecs::world::World;
use campfire_common::ExitStatus;

use crate::faults::Faults;
use crate::faults::fault::{Fault, FaultPolicy};
use crate::sim_client::receipt_writer::ReceiptWriter;
use crate::sim_server::journal_watch::JournalWatch;

/// What the faults module gives a world, for its own crate.
pub(crate) trait FaultsWorldCrate {
    /// Takes the failures of the session's workers, the journal's and the receipt writer's,
    /// with those reported, and applies each policy: logs each, and gives the exit of a server
    /// one of them ends.
    fn apply_faults(&mut self) -> Option<AppExit>;
}

impl FaultsWorldCrate for World {
    fn apply_faults(&mut self) -> Option<AppExit> {
        let mut gathered = Vec::new();
        if let Some(error) = self
            .get_resource::<JournalWatch>()
            .and_then(|journal| journal.0.take_failure())
        {
            gathered.push(Fault::Journal(error));
        }
        if let Some(writer) = self.get_resource::<ReceiptWriter>() {
            gathered.extend(writer.failures().into_iter().map(Fault::Receipt));
        }
        let mut faults = self.resource_mut::<Faults>();
        faults.pending.extend(gathered);
        let mut exit = None;
        for fault in faults.pending.drain(..) {
            fault.log();
            match fault.source().policy() {
                FaultPolicy::EndServer => {
                    exit = Some(AppExit::from_code(ExitStatus::Storage.code()));
                }
                FaultPolicy::Log => {}
            }
        }
        exit
    }
}
