use bevy_app::AppExit;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

use crate::faults::fault::{Fault, FaultPolicy};
use crate::journal_watch::JournalWatch;
use crate::server_exit::ServerExit;
use crate::sim_client::receipt_writer::ReceiptWriter;

pub(crate) mod fault;

/// The one surface of the workers' failures, on a server and on a client: each worker's failure
/// reaches it with its source, and `apply` acts on each as the table of policies says, in place
/// of each worker's own report. A failure before the app runs, in a restore's abort or a local
/// server's start, is the error of the function that met it.
#[derive(Resource, Debug, Default)]
pub(crate) struct Faults {
    /// The faults reported and not applied yet, in the order reported.
    pending: Vec<Fault>,
}

impl Faults {
    pub(crate) fn report(&mut self, fault: Fault) {
        self.pending.push(fault);
    }

    /// Takes the failures of the workers in `world`, the journal's and the receipt writer's,
    /// with those reported, and applies each policy: logs each, and gives the exit of a server
    /// one of them ends.
    pub(crate) fn apply(world: &mut World) -> Option<AppExit> {
        let mut gathered = Vec::new();
        if let Some(error) = world
            .get_resource::<JournalWatch>()
            .and_then(|journal| journal.0.take_failure())
        {
            gathered.push(Fault::Journal(error));
        }
        if let Some(writer) = world.get_resource::<ReceiptWriter>() {
            gathered.extend(writer.failures().into_iter().map(Fault::Receipt));
        }
        let mut faults = world.resource_mut::<Faults>();
        faults.pending.extend(gathered);
        let mut exit = None;
        for fault in faults.pending.drain(..) {
            fault.log();
            match fault.source().policy() {
                FaultPolicy::EndServer => {
                    exit = Some(AppExit::from_code(ServerExit::STORAGE_FAILED));
                }
                FaultPolicy::Log => {}
            }
        }
        exit
    }

    /// A client's system: applies the faults of the frame, and exits as they say.
    pub(crate) fn watch(world: &mut World) {
        if let Some(exit) = Faults::apply(world) {
            world.write_message(exit);
        }
    }
}

#[cfg(test)]
mod tests;
