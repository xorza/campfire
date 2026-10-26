use crate::deficit::Deficit;
use crate::sync_adjustment::SyncAdjustment;
use crate::sync_setup::SyncSetup;
use bevy_reflect::Reflect;
use campfire_math::Rounding;
use campfire_net_core::TickSpan;

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
    /// The adjustment for an error of `offset` ticks. Any error behind the objective steps to it at
    /// once when `deficit` says so, as each input it sends meanwhile goes later, so its band lies
    /// ahead of its objective alone (11-net-stack/time.md, A client behind steps, a client ahead
    /// slews; 11-net-stack/time.md, The input timeline's band lies ahead of its objective). Any
    /// other error past the band adjusts the speed only once it repeats on one side of the
    /// objective: a sign flip starts a new run, since errors on both sides are noise, not drift.
    pub(crate) fn speed_adjustment(
        &mut self,
        setup: &SyncSetup,
        offset: TickSpan,
        deficit: Deficit,
    ) -> SyncAdjustment {
        let sign = !offset.is_negative();
        let flipped = sign != self.previous_error_sign;
        self.previous_error_sign = sign;
        let error = offset.unsigned_abs();
        if error > setup.max_error_margin {
            self.consecutive_errors = 0;
            return SyncAdjustment::Resync;
        }
        if offset.is_negative() && deficit == Deficit::Step {
            self.consecutive_errors = 0;
            return SyncAdjustment::Resync;
        }
        if error <= setup.error_margin {
            self.consecutive_errors = 0;
            return SyncAdjustment::Nominal;
        }
        self.consecutive_errors = if flipped {
            1
        } else {
            self.consecutive_errors.saturating_add(1)
        };
        if self.consecutive_errors < setup.consecutive_errors_threshold {
            return SyncAdjustment::Nominal;
        }
        let base_factor = setup.speedup_factor - 1.0;
        // At most 1, so its 24 bits fit an `f32` exactly, and the band's bounds keep the divisor
        // above 0.
        let error_ratio = error
            .value()
            .checked_div(setup.max_error_margin.value(), Rounding::NearestEven)
            .expect("the band keeps the largest error above 0");
        #[expect(
            clippy::cast_precision_loss,
            reason = "a ratio of at most 1 has 24 bits"
        )]
        let error_ratio = error_ratio.to_bits() as f32 / 16_777_216.0;
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
    use campfire_math::Num;

    use super::*;

    /// With a threshold of 3, two errors ahead then two behind never slew: the flip restarts the
    /// run at 1, and a third error behind completes it, 1 + 0.05 × 2 × 2 / 10 = 1.02. A timeline
    /// that steps a deficit steps at the first error behind, every time, half a tick within the
    /// band as well, where the other runs at its speed; within the band ahead both run at it. A
    /// run ahead slows the same in both, to 1 / 1.02.
    #[test]
    fn a_sign_flip_restarts_the_run_and_a_deficit_steps_at_once_or_slews() {
        let setup = SyncSetup {
            consecutive_errors_threshold: 3,
            ..SyncSetup::default()
        };
        let mut slews = SyncHysteresis::default();
        for offset in [2, 2, -2, -2].map(TickSpan::from) {
            assert!(matches!(
                slews.speed_adjustment(&setup, offset, Deficit::Slew),
                SyncAdjustment::Nominal
            ));
        }
        let third = slews.speed_adjustment(&setup, TickSpan::from(-2), Deficit::Slew);
        assert!(matches!(third, SyncAdjustment::SpeedAdjust(1.02)));
        let mut steps = SyncHysteresis::default();
        let adjusted = [2, 2, -2, -2]
            .map(|offset| steps.speed_adjustment(&setup, TickSpan::from(offset), Deficit::Step));
        assert!(matches!(
            adjusted,
            [
                SyncAdjustment::Nominal,
                SyncAdjustment::Nominal,
                SyncAdjustment::Resync,
                SyncAdjustment::Resync
            ]
        ));
        let half = TickSpan::try_from(Num::HALF).unwrap();
        let within =
            |deficit, offset| SyncHysteresis::default().speed_adjustment(&setup, offset, deficit);
        let behind = [Deficit::Step, Deficit::Slew].map(|deficit| within(deficit, -half));
        assert!(matches!(
            behind,
            [SyncAdjustment::Resync, SyncAdjustment::Nominal]
        ));
        for deficit in [Deficit::Slew, Deficit::Step] {
            assert!(matches!(
                within(deficit, TickSpan::from(1)),
                SyncAdjustment::Nominal
            ));
            let mut hysteresis = SyncHysteresis::default();
            let ahead = [2, 2, 2]
                .map(|offset| hysteresis.speed_adjustment(&setup, TickSpan::from(offset), deficit));
            assert!(matches!(ahead[2], SyncAdjustment::SpeedAdjust(ratio) if ratio == 1.0 / 1.02));
        }
    }
}
