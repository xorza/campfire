use bevy_ecs::event::Event;
use campfire_math::TickRate;

/// Event that changes the timelines' rate: the [`TimelineRate`](crate::TimelineRate) resource, and
/// the timestep of [`Time<Fixed>`](bevy_time::Time) derived from it.
#[derive(Debug, Event)]
pub struct SetTickRate(pub TickRate);
