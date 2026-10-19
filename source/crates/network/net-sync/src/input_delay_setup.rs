use crate::link_estimate::LinkEstimate;
use crate::sync_setup::SyncSetup;
use bevy_reflect::Reflect;
use campfire_math::TickRate;
use core::time::Duration;

#[derive(Debug, Clone, Copy, Reflect)]
pub struct InputDelaySetup {
    /// Minimum number of input delay ticks that will be applied, regardless of latency.
    ///
    /// This should almost always be set to 0 to ensure that your game is as responsive as possible.
    /// Some games might prefer enforcing a minimum input delay to ensure a consistent game feel
    /// even when the latency conditions are changing.
    min_input_delay_ticks: u16,
    /// Maximum amount of input delay that will be applied in order to cover latency, before any
    /// prediction is done to cover additional latency.
    ///
    /// Input delay can be ideal in low-latency situations to avoid rollbacks and networking
    /// artifacts, but it must be balanced against the responsiveness of the game. Even at higher
    /// latencies, it's useful to add some input delay to reduce the amount of rollback ticks that
    /// are needed. (to reduce the rollback visual artifacts and CPU costs)
    ///
    /// The default value is 3 (or about 50ms at 60Hz): for clients that have less than 50ms ping,
    /// we will apply input delay to cover the latency, and there should no rollback.
    ///
    /// Set to 0ms if you won't want any input delay. (for example for shooters)
    max_input_delay_before_prediction: u16,
    /// This setting describes how far ahead the client simulation is allowed to predict to cover
    /// latency. This controls the maximum amount of rollback ticks. Any additional latency will be
    /// covered by adding more input delays.
    ///
    /// The default value is 7 ticks (or about 100ms of prediction at 60Hz)
    ///
    /// If you set `max_input_delay_before_prediction` to 50ms and `max_predicted_time` to 100ms,
    /// and the client has:
    /// - 30ms ping: there will be 30ms of input delay and no prediction
    /// - 120ms ping: there will be 50ms of input delay and 70ms of prediction/rollback
    /// - 200ms ping: there will be 100ms of input delay, and 100ms of prediction/rollback
    max_predicted_ticks: u16,
}

impl InputDelaySetup {
    /// The setup of these bounds, each in ticks.
    ///
    /// # Panics
    ///
    /// When the minimum input delay exceeds the maximum before prediction.
    pub const fn new(
        min_input_delay_ticks: u16,
        max_input_delay_before_prediction: u16,
        max_predicted_ticks: u16,
    ) -> Self {
        assert!(
            min_input_delay_ticks <= max_input_delay_before_prediction,
            "the minimum input delay exceeds the maximum before prediction"
        );
        Self {
            min_input_delay_ticks,
            max_input_delay_before_prediction,
            max_predicted_ticks,
        }
    }

    pub const fn min_input_delay_ticks(&self) -> u16 {
        self.min_input_delay_ticks
    }

    pub const fn max_input_delay_before_prediction(&self) -> u16 {
        self.max_input_delay_before_prediction
    }

    pub const fn max_predicted_ticks(&self) -> u16 {
        self.max_predicted_ticks
    }

    /// Cover up to 50ms of latency with input delay, and after that use prediction for up to 100ms
    /// - `min_input_delay_ticks`: no minimum input delay
    /// - `min_input_delay_before_prediction`: 3 ticks (or about 50ms at 60Hz), cover 50ms of
    ///   latency with input delay
    /// - `max_predicted_ticks`: 7 ticks (or about 100ms at 60Hz), cover the next 100ms of latency
    ///   with prediction (the rest will be covered by more input delay)
    pub const fn balanced() -> Self {
        Self {
            min_input_delay_ticks: 0,
            max_input_delay_before_prediction: 3,
            max_predicted_ticks: 7,
        }
    }

    /// No input-delay, all the latency will be covered by prediction. We have unlimited prediction
    pub const fn no_input_delay() -> Self {
        Self {
            min_input_delay_ticks: 0,
            max_input_delay_before_prediction: 0,
            max_predicted_ticks: 100,
        }
    }

    pub const fn fixed_input_delay(delay_ticks: u16) -> Self {
        Self {
            min_input_delay_ticks: delay_ticks,
            max_input_delay_before_prediction: delay_ticks,
            max_predicted_ticks: 100,
        }
    }

    /// Compute the amount of input delay that should be applied, considering the link's
    /// estimate; with none yet, an RTT and a jitter of zero.
    pub(crate) fn input_delay_ticks(
        self,
        estimate: Option<LinkEstimate>,
        sync_setup: &SyncSetup,
        tick_rate: TickRate,
    ) -> u16 {
        let (rtt, jitter) = estimate.map_or((Duration::ZERO, Duration::ZERO), |estimate| {
            (estimate.rtt(), estimate.jitter())
        });
        let jitter_margin = sync_setup.jitter_margin(jitter, tick_rate);
        let effective_rtt = rtt + jitter_margin;
        let rtt_ticks = u16::try_from(tick_rate.ticks_covering(effective_rtt)).unwrap_or(u16::MAX);
        // if the rtt is lower than the minimum input delay, we will apply the minimum input delay
        if rtt_ticks <= self.min_input_delay_ticks {
            return self.min_input_delay_ticks;
        }
        // else, apply input delay up to the maximum input delay
        if rtt_ticks <= self.max_input_delay_before_prediction {
            return rtt_ticks;
        }
        // else, apply input delay up to the maximum input delay, and cover the rest with prediction
        // if not possible, add even more input delay
        if rtt_ticks
            <= self
                .max_predicted_ticks
                .saturating_add(self.max_input_delay_before_prediction)
        {
            self.max_input_delay_before_prediction
        } else {
            rtt_ticks - self.max_predicted_ticks
        }
    }
}
