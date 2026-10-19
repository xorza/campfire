use crate::sync_adjustment::SyncAdjustment;
use crate::sync_config::SyncConfig;
use bevy_reflect::Reflect;
use campfire_net_core::TickDelta;

/// Mutable hysteresis shared by timeline synchronization controllers.
///
/// This type is public because synchronization policies live in separate stack crates. Its
/// fields and adjustment algorithm remain private; applications normally interact with a
/// `TimelineSync` implementation instead.
#[derive(Debug, Reflect)]
pub struct SyncHysteresis {
    /// How many consecutive errors have we seen that are in the same direction
    consecutive_errors: u8,
    /// Sign of the previous error
    previous_error_sign: bool,
}

impl Default for SyncHysteresis {
    fn default() -> Self {
        Self {
            consecutive_errors: 0,
            previous_error_sign: true,
        }
    }
}

impl SyncHysteresis {
    /// The adjustment for an error of `offset` ticks. An error past the band adjusts the speed
    /// only once it repeats on one side of the objective: a sign flip starts a new run, since
    /// errors on both sides are noise, not drift.
    pub(crate) fn speed_adjustment(
        &mut self,
        config: &SyncConfig,
        offset: TickDelta,
    ) -> SyncAdjustment {
        let sign = !offset.is_negative();
        let flipped = sign != self.previous_error_sign;
        self.previous_error_sign = sign;
        let error = offset.unsigned_abs();
        if error > config.max_error_margin {
            self.consecutive_errors = 0;
            return SyncAdjustment::Resync;
        }
        if error <= config.error_margin {
            self.consecutive_errors = 0;
            return SyncAdjustment::Nominal;
        }
        self.consecutive_errors = if flipped {
            1
        } else {
            self.consecutive_errors.saturating_add(1)
        };
        if self.consecutive_errors < config.consecutive_errors_threshold {
            return SyncAdjustment::Nominal;
        }
        let base_factor = config.speedup_factor - 1.0;
        // At most 1, and the band's bounds keep the divisor above 0.
        let error_ratio: f32 = (error.value / config.max_error_margin.value).to_num();
        // Apply progressively stronger adjustment as error increases
        let adjustment = 1.0 + (base_factor * error_ratio * 2.0);
        // Slow down if we are ahead
        let ratio = if offset.is_positive() {
            1.0 / adjustment
        } else {
            adjustment
        };
        SyncAdjustment::SpeedAdjust(ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With a threshold of 3, two errors ahead then two behind never adjust: the flip restarts
    /// the run at 1. A third error behind completes it.
    #[test]
    fn a_sign_flip_restarts_the_run() {
        let config = SyncConfig {
            consecutive_errors_threshold: 3,
            ..SyncConfig::default()
        };
        let mut hysteresis = SyncHysteresis::default();
        for offset in [2, 2, -2, -2].map(TickDelta::from) {
            assert!(matches!(
                hysteresis.speed_adjustment(&config, offset),
                SyncAdjustment::Nominal
            ));
        }
        assert!(matches!(
            hysteresis.speed_adjustment(&config, TickDelta::from(-2)),
            SyncAdjustment::SpeedAdjust(_)
        ));
    }
}
