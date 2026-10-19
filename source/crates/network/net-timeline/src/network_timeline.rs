use crate::timeline::Timeline;
use crate::timeline_setup::TimelineSetup;
use bevy_ecs::component::{Component, Mutable};
use campfire_net_core::NetTick;
use campfire_net_core::Overstep;
use campfire_net_core::TickInstant;
use campfire_net_core::TickRate;
use campfire_net_core::TickSpan;
use core::ops::DerefMut;
use core::time::Duration;

/// A component that wraps a [`Timeline`], whose `now` moves by fractional [`TickSpan`]s.
pub trait NetworkTimeline: Component<Mutability = Mutable> {
    type Setup: TimelineSetup;

    /// Estimate of the current time in the [`Timeline`]
    fn now(&self) -> TickInstant;

    fn tick(&self) -> NetTick;

    fn overstep(&self) -> Overstep;

    fn set_now(&mut self, now: TickInstant);

    fn apply_delta(&mut self, delta: TickSpan);

    fn apply_duration(&mut self, duration: Duration, rate: TickRate) {
        self.apply_delta(TickSpan::from_duration(duration, rate));
    }
}

impl<C: TimelineSetup, T: Component<Mutability = Mutable> + DerefMut<Target = Timeline<C>>>
    NetworkTimeline for T
{
    type Setup = C;

    /// Estimate of the current time in the [`Timeline`]
    fn now(&self) -> TickInstant {
        self.now
    }

    fn tick(&self) -> NetTick {
        self.now().tick()
    }

    fn overstep(&self) -> Overstep {
        self.now().overstep()
    }

    fn set_now(&mut self, now: TickInstant) {
        self.now = now;
    }

    fn apply_delta(&mut self, delta: TickSpan) {
        self.now = self.now + delta;
    }
}
