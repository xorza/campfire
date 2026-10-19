use bevy_ecs::resource::Resource;
use campfire_math::TickRate;

/// The rate the app's timelines tick at, which [`SetTickRate`](crate::SetTickRate) sets: a paced
/// local match runs its timelines at the match's rate times its speed, while the sim keeps the
/// match's own (14-workspace-alignment.md, One tick rate type, two resources).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineRate(pub TickRate);
