use crate::link_estimate::LinkEstimate;
use crate::sync_setup::SyncSetup;
use bevy_reflect::Reflect;
use campfire_math::TickRate;
use core::time::Duration;

/// How far a client delays its inputs, and how far it predicts, each in ticks.
#[derive(Debug, Clone, Copy, Reflect)]
pub struct InputDelaySetup {
    /// The fewest ticks of input delay, whatever the latency: 0 for the most responsive game; more
    /// for a game that keeps one feel as the latency changes.
    min_input_delay_ticks: u16,
    /// The most ticks of input delay that cover the latency before prediction covers the rest:
    /// delay avoids rollbacks at low latency, and costs responsiveness.
    max_input_delay_before_prediction: u16,
    /// The most ticks the client predicts to cover the latency, the bound of its rollbacks; more
    /// delay covers any latency past it.
    max_predicted_ticks: u16,
}

impl InputDelaySetup {
    pub const fn min_input_delay_ticks(&self) -> u16 {
        self.min_input_delay_ticks
    }

    pub const fn max_input_delay_before_prediction(&self) -> u16 {
        self.max_input_delay_before_prediction
    }

    pub const fn max_predicted_ticks(&self) -> u16 {
        self.max_predicted_ticks
    }

    /// No input delay: prediction covers all the latency, up to 100 ticks.
    pub const fn no_input_delay() -> Self {
        Self {
            min_input_delay_ticks: 0,
            max_input_delay_before_prediction: 0,
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

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::input_delay_setup::InputDelaySetup;

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

        /// Delay that covers up to 3 ticks of latency, then prediction for up to 7 more.
        pub const fn balanced() -> Self {
            Self {
                min_input_delay_ticks: 0,
                max_input_delay_before_prediction: 3,
                max_predicted_ticks: 7,
            }
        }

        /// A delay of `delay_ticks` ticks whatever the latency, and prediction for up to 100.
        pub const fn fixed_input_delay(delay_ticks: u16) -> Self {
            Self {
                min_input_delay_ticks: delay_ticks,
                max_input_delay_before_prediction: delay_ticks,
                max_predicted_ticks: 100,
            }
        }
    }
}
