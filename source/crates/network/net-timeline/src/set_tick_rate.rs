use bevy_ecs::event::Event;
use campfire_net_core::TickRate;

/// Event that changes the tick rate: the [`TickRate`] resource, and the timestep of
/// [`Time<Fixed>`](bevy_time::Time) derived from it.
#[derive(Debug, Event)]
pub struct SetTickRate(pub TickRate);
