use bevy_app::AppExit;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::ExitStatus;

use crate::faults::fault::{Fault, FaultPolicy};

pub(crate) mod error;
pub(crate) mod fault;

/// The one surface of the workers' failures, on a server and on a client: each worker's failure
/// reaches it with its source, and `apply` acts on each as the table of policies says, in place
/// of each worker's own report. A failure before the app runs, in a restore's abort or a local
/// server's start, is the error of the function that met it.
#[derive(Resource, Debug, Default)]
pub struct WorkerFaults {
    /// The faults reported and not applied yet, in the order reported.
    pending: Vec<Fault>,
}

impl WorkerFaults {
    pub fn report(&mut self, fault: Fault) {
        self.pending.push(fault);
    }

    /// A client's system: applies the faults of the frame, and exits as they say.
    pub fn watch(world: &mut World) {
        if let Some(exit) = Self::apply(world) {
            world.write_message(exit);
        }
    }

    /// Applies the policy of each fault reported: logs each, and gives the exit of a server one
    /// of them ends.
    pub fn apply(world: &mut World) -> Option<AppExit> {
        let mut faults = world.resource_mut::<Self>();
        let mut exit = None;
        for fault in faults.pending.drain(..) {
            fault.log();
            match fault.policy() {
                FaultPolicy::EndServer => {
                    exit = Some(AppExit::from_code(ExitStatus::Storage.code()));
                }
                FaultPolicy::Log => {}
            }
        }
        exit
    }
}

#[cfg(test)]
mod tests;
