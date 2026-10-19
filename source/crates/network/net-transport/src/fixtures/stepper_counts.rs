use bevy_ecs::world::World;
use campfire_net_channels::internals::TransportCounts;
use campfire_net_stepper::Stepper;

use crate::transport::Transport;

/// The counters of every transport in a stepper's apps, summed.
pub fn summed_transport_counts<S>(stepper: &mut Stepper<S>) -> TransportCounts {
    stepper
        .apps_mut()
        .fold(TransportCounts::default(), |counts, app| {
            counts + of_world(app.world_mut())
        })
}

fn of_world(world: &mut World) -> TransportCounts {
    world
        .query::<&Transport>()
        .iter(world)
        .fold(TransportCounts::default(), |counts, transport| {
            counts + transport.counts()
        })
}
