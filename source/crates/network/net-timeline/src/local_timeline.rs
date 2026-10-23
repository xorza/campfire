use campfire_net_core::NetTick;
use campfire_net_core::Overstep;
use campfire_net_core::TickInstant;

use bevy_ecs::prelude::Resource;
use bevy_ecs::system::ResMut;
use bevy_reflect::Reflect;
use bevy_time::{Fixed, Time};
use tracing::trace;

/// The local simulation timeline: its tick advances each `FixedUpdate`, rollback replays included,
/// and its fraction is the overstep of [`Time<Fixed>`] (11-net-stack/time.md, Ticks stay on Bevy's
/// clocks).
#[derive(Debug, Resource, Default, Clone, Reflect)]
pub struct LocalTimeline {
    tick: NetTick,
    /// Whether `FixedMain` simulates [`Self::tick`] now: from its tick's advance in
    /// `FixedFirst` to the end of its `FixedLast`.
    simulating: bool,
}

impl LocalTimeline {
    /// The current tick.
    pub const fn tick(&self) -> NetTick {
        self.tick
    }

    /// The tick `FixedMain` simulates now, a rollback replay included, or `None` outside it.
    /// It is the one tick a client and its server agree on for an entity spawned in it: outside
    /// `FixedMain`, [`Self::tick`] is the last one simulated.
    pub const fn simulated_tick(&self) -> Option<NetTick> {
        if self.simulating {
            Some(self.tick)
        } else {
            None
        }
    }

    /// The current fractional simulation instant.
    ///
    /// [`LocalTimeline`] holds the whole simulation tick while Bevy's [`Time<Fixed>`]
    /// holds the fractional progress towards the next fixed update. Constructing the
    /// instant on demand avoids maintaining a second, potentially stale clock.
    pub fn now(&self, fixed_time: &Time<Fixed>) -> TickInstant {
        TickInstant::from_tick_and_overstep(
            self.tick,
            Overstep::of(fixed_time.overstep(), fixed_time.timestep()),
        )
    }

    /// Moves the local tick by `ticks` whole ticks, back when negative.
    pub fn shift(&mut self, ticks: i32) {
        self.tick = self.tick + ticks;
    }

    /// Increment the local tick at each FixedUpdate
    pub(crate) fn increment_tick(mut timeline: ResMut<'_, Self>) {
        timeline.tick += 1;
        timeline.simulating = true;
        trace!(
            target: "campfire_net_debug::timeline",
            kind = "local_tick",
            local_tick = timeline.tick.0,
            "local timeline tick advanced"
        );
    }

    /// Ends the simulation of the local tick, after the `FixedLast` that closes it.
    pub(crate) fn end_tick(mut timeline: ResMut<'_, Self>) {
        timeline.simulating = false;
    }
}
