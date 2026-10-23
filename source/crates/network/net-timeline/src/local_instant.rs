use bevy_ecs::system::{Res, SystemParam};
use bevy_time::{Fixed, Time};
use campfire_net_core::TickInstant;

use crate::local_timeline::LocalTimeline;

/// The local timeline with Bevy's fixed clock, whose overstep is its fraction: what reads the
/// timeline's instant, not only its tick, as a packet's sender stamps it.
#[derive(SystemParam, Debug)]
pub struct LocalInstant<'w> {
    timeline: Res<'w, LocalTimeline>,
    fixed_time: Res<'w, Time<Fixed>>,
}

impl LocalInstant<'_> {
    pub fn now(&self) -> TickInstant {
        self.timeline.now(&self.fixed_time)
    }
}
