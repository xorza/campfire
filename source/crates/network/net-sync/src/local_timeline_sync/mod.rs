use crate::deficit::Deficit;
use crate::sync_hysteresis::SyncHysteresis;
use crate::sync_sample::SyncSample;
use crate::sync_setup::SyncSetup;
use crate::timeline_sync::TimelineSync;

use crate::input_timeline_setup::InputTimelineSetup;
use crate::link_estimate::LinkEstimate;
use crate::link_estimator::LinkEstimator;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_math::TickRate;
use campfire_net_connection::NetworkTopology;
use campfire_net_connection::NetworkingState;
use campfire_net_core::TickInstant;
use campfire_net_core::TickSpan;
use campfire_net_timeline::TimelineRate;
use tracing::trace;

/// Runtime state for synchronizing the application-global
/// [`LocalTimeline`](campfire_net_timeline::LocalTimeline).
///
/// This resource is not a clock. The authoritative fractional simulation instant is constructed
/// from `LocalTimeline` and `Time<Fixed>` when synchronization runs.
#[derive(Resource, Debug, Reflect)]
pub struct LocalTimelineSync {
    controller: SyncHysteresis,
    /// Current input_delay_ticks that are being applied
    pub(crate) input_delay_ticks: u16,
    /// The input delay the link's latency calls for, which the next resync applies: a delay
    /// changes only with the timeline shift a resync makes, as inputs are stamped by it.
    link_input_delay_ticks: u16,
    relative_speed: f32,
    is_synced: bool,
}

impl LocalTimelineSync {
    /// The input delay, in ticks.
    pub const fn input_delay(&self) -> u16 {
        self.input_delay_ticks
    }
}

impl Default for LocalTimelineSync {
    fn default() -> Self {
        Self {
            controller: SyncHysteresis::default(),
            input_delay_ticks: 0,
            link_input_delay_ticks: 0,
            relative_speed: 1.0,
            is_synced: false,
        }
    }
}

impl LocalTimelineSync {
    /// Recompute the global input delay from the client Link of the current topology, and
    /// apply it at once.
    ///
    /// Returns `false` when the topology has no client Link.
    pub(crate) fn recompute_input_delay(
        &mut self,
        setup: &InputTimelineSetup,
        topology: &NetworkTopology,
        links: &Query<'_, '_, &LinkEstimator>,
        tick_rate: TickRate,
    ) -> bool {
        let client = match topology {
            NetworkTopology::Client(client) => *client,
            NetworkTopology::Undefined
            | NetworkTopology::Server(_)
            | NetworkTopology::Invalid(_) => return false,
        };
        let Ok(estimator) = links.get(client) else {
            return false;
        };
        // At once, where no resync will.
        self.set_link_input_delay(setup, estimator.estimate(), tick_rate);
        self.input_delay_ticks = self.link_input_delay_ticks;
        true
    }

    /// Recompute input delay when the global configuration resource is inserted or replaced.
    pub(crate) fn recompute_input_delay_on_setup_update(
        _trigger: On<'_, '_, Insert<InputTimelineSetup>>,
        tick_rate: Res<'_, TimelineRate>,
        networking: Res<'_, NetworkingState>,
        links: Query<'_, '_, &LinkEstimator>,
        setup: Res<'_, InputTimelineSetup>,
        mut timeline: ResMut<'_, Self>,
    ) {
        if !timeline.recompute_input_delay(&setup, &networking.topology, &links, tick_rate.0) {
            // Configuration is commonly installed before a Client connects. The
            // configured minimum is topology-independent; the first resync replaces it with the
            // latency-aware delay once the Link is ready.
            timeline.input_delay_ticks = setup.input_delay_setup.min_input_delay_ticks();
        }
        trace!(
            input_delay_ticks = timeline.input_delay(),
            setup = ?setup.input_delay_setup,
            topology = ?networking.topology,
            "recomputed global input delay after config update"
        );
    }

    /// Records the input delay the link's `estimate` calls for, which the next resync applies.
    pub(crate) fn set_link_input_delay(
        &mut self,
        setup: &InputTimelineSetup,
        estimate: Option<LinkEstimate>,
        tick_rate: TickRate,
    ) {
        self.link_input_delay_ticks =
            setup
                .input_delay_setup
                .input_delay_ticks(estimate, &setup.sync, tick_rate);
    }
}

impl TimelineSync for LocalTimelineSync {
    type Setup = InputTimelineSetup;

    /// A timeline behind sends late inputs, which the server applies late or drops, so it steps;
    /// one ahead costs only latency, so it slews.
    const DEFICIT: Deficit = Deficit::Step;

    fn sync_hysteresis(&mut self) -> &mut SyncHysteresis {
        &mut self.controller
    }

    fn sync_setup(setup: &Self::Setup) -> &SyncSetup {
        &setup.sync
    }

    /// Compute the desired local simulation instant.
    ///
    /// The local simulation runs far enough ahead of the remote timeline for inputs to arrive,
    /// with RTT and jitter margins. Input delay reduces that required clock lead because inputs are
    /// assigned to later ticks without advancing the simulation clock itself.
    fn sync_objective(&self, sample: &SyncSample<'_, Self::Setup>) -> TickInstant {
        let SyncSample {
            remote,
            setup,
            estimate,
            tick_rate,
        } = *sample;
        let remote = remote.current_estimate();
        let network_delay = TickSpan::from_duration(estimate.rtt() / 2, tick_rate);
        let jitter_margin = TickSpan::from_duration(
            setup.sync.jitter_margin(estimate.jitter(), tick_rate),
            tick_rate,
        );
        let input_delay = TickSpan::from(i32::from(self.input_delay_ticks));
        // Inputs received by the server in `PreUpdate` are first read in
        // `FixedPreUpdate`, after `LocalTimeline` advances in `FixedFirst`.
        // Therefore an input packet that arrives while the server is at tick T
        // must contain inputs for at least tick T + 1; one tick more keeps it in
        // time through a stall that wakes every process together, which takes the
        // phase to the server's next tick away (11-net-stack/time.md, An input
        // leads by one margin for each reason).
        let obj = remote + network_delay + jitter_margin + TickSpan::from(2) - input_delay;
        trace!(
            ?remote,
            ?network_delay,
            ?jitter_margin,
            ?input_delay,
            "LocalTimeline sync objective: {:?}",
            obj
        );
        obj
    }

    /// Applies the link's input delay with the shift, in one step: the objective subtracts the
    /// applied delay, so the new one moves it by their difference.
    fn resync(&mut self, now: TickInstant, sync_objective: TickInstant) -> i32 {
        let objective = sync_objective
            + TickSpan::from(
                i32::from(self.input_delay_ticks) - i32::from(self.link_input_delay_ticks),
            );
        self.input_delay_ticks = self.link_input_delay_ticks;
        // Up, so the timeline lands at its objective or ahead of it, inside its band
        // (11-net-stack/time.md, The input timeline's band lies ahead of its objective).
        (objective - now).to_i32_ceil()
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

    /// Resets the controller and the link's input delay, which the first resync sets.
    fn reset(&mut self) {
        trace!("Resetting LocalTimelineSync");
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests;
