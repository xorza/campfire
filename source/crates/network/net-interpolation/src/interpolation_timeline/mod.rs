//! The interpolation timeline, which trails the remote timeline by a delay that the sender's
//! replication interval sets: a sender announces it in its `SenderHello` when it connects.
use core::time::Duration;

use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_core::TickInstant;
use campfire_net_core::TickSpan;
use campfire_net_sync::Deficit;
use campfire_net_sync::SyncHysteresis;
use campfire_net_sync::SyncSample;
use campfire_net_sync::SyncSetup;
use campfire_net_sync::TimelineSync;
use campfire_net_timeline::Timeline;
use tracing::trace;

use crate::interpolation_setup::InterpolationSetup;

/// The timeline the interpolation histories are sampled at: it trails the remote timeline at a
/// speed its sync controls, apart from the local one (11-net-stack/time.md, Two estimates of one
/// link keep two promises).
#[derive(Debug, Resource, Default, Deref, DerefMut, Reflect)]
pub struct InterpolationTimeline(Timeline<InterpolationSetup>);

impl TimelineSync for InterpolationTimeline {
    type Setup = InterpolationSetup;

    /// A step would show as a jump, and a presentation behind only shows older states.
    const DEFICIT: Deficit = Deficit::Slew;

    fn sync_hysteresis(&mut self) -> &mut SyncHysteresis {
        &mut self.state.hysteresis
    }

    fn sync_setup(setup: &Self::Setup) -> &SyncSetup {
        &setup.sync
    }

    fn sync_objective(&self, sample: &SyncSample<'_, InterpolationSetup>) -> TickInstant {
        let SyncSample {
            remote,
            setup,
            estimate,
            tick_rate,
        } = *sample;
        let delay =
            TickSpan::from_duration(setup.to_duration(self.remote_send_interval), tick_rate);
        // take extra margin if there is jitter
        let jitter_margin = TickSpan::from_duration(
            setup.sync.jitter_margin(estimate.jitter(), tick_rate),
            tick_rate,
        );
        let target = remote.current_estimate();
        let obj = target - (delay + jitter_margin);
        trace!(
            ?target,
            ?delay,
            ?jitter_margin,
            send_interval = ?self.remote_send_interval,
            "InterpolationTimeline objective: {:?}", obj
        );
        obj
    }

    fn resync(&mut self, now: TickInstant, sync_objective: TickInstant) -> i32 {
        let target = sync_objective;
        let tick_span = (target - now).to_i32();
        trace!(?tick_span, "Resync Interpolation timeline!");
        self.now = target;
        tick_span
    }

    fn is_synced(&self) -> bool {
        self.is_synced
    }

    fn set_synced(&mut self, synced: bool) {
        self.is_synced = synced;
    }

    fn relative_speed(&self) -> f32 {
        self.relative_speed
    }

    fn set_relative_speed(&mut self, ratio: f32) {
        self.relative_speed = ratio;
    }

    /// Resets the synchronization controller, and keeps the presentation cursor.
    fn reset(&mut self) {
        self.hysteresis = SyncHysteresis::default();
        self.is_synced = false;
        self.relative_speed = 1.0;
    }
}

impl InterpolationTimeline {
    /// Resets the timeline for a new link: the controller, the presentation cursor and the remote
    /// send interval the previous link advertised.
    pub(crate) fn reset_link(&mut self) {
        self.reset();
        self.now = TickInstant::default();
        self.remote_send_interval = Duration::default();
    }
}

#[cfg(test)]
mod tests;
