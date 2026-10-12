use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

use crate::faults::fault::Fault;
use crate::faults::faults_world::FaultsWorldCrate;

pub(crate) mod fault;
pub(crate) mod faults_world;

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

    /// A client's system: applies the faults of the frame, and exits as they say.
    pub(crate) fn watch(world: &mut World) {
        if let Some(exit) = world.apply_faults() {
            world.write_message(exit);
        }
    }
}

#[cfg(test)]
mod tests;
