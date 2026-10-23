//! The blend context of a delayed sample, from the ticks of the history entries it brackets.

use campfire_math::{Num, TickRate};
use campfire_net_core::NetTick;
use campfire_net_frame_interpolation::InterpolationSampleCx;

/// The context of a sample at `interpolation_tick` plus `interpolation_overstep` between the
/// entries at `start_tick` and `end_tick`, the interval timed by `tick_rate` when known.
pub(crate) fn sample_of_ticks(
    start_tick: NetTick,
    end_tick: NetTick,
    interpolation_tick: NetTick,
    interpolation_overstep: f32,
    tick_rate: Option<TickRate>,
) -> InterpolationSampleCx {
    let t = interpolation_fraction(
        start_tick,
        end_tick,
        interpolation_tick,
        interpolation_overstep,
    )
    .unwrap_or(0.0);
    let sample_delta_secs =
        tick_rate.and_then(|tick_rate| sample_delta_secs(start_tick, end_tick, tick_rate));
    InterpolationSampleCx::new(t, sample_delta_secs)
}

fn sample_delta_secs(start_tick: NetTick, end_tick: NetTick, tick_rate: TickRate) -> Option<f32> {
    let ticks = end_tick - start_tick;
    (ticks > 0).then(|| {
        tick_rate
            .duration_of(Num::int(i64::from(ticks)))
            .as_secs_f32()
    })
}

/// How far `current` plus `overstep` ticks is from `start` to `end`, clamped to `[0, 1]`: `None`
/// when `end` is not after `start`, which brackets no interval.
fn interpolation_fraction(
    start: NetTick,
    end: NetTick,
    current: NetTick,
    overstep: f32,
) -> Option<f32> {
    let interval = end - start;
    (interval > 0)
        .then(|| (((current - start) as f32 + overstep) / interval as f32).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// From 10 to 20: tick 15 plus 0.5 is (5 + 0.5) / 10 = 0.55 of the way; 5 and 25 clamp to 0 and
    /// 1; an interval that ends at or before its start brackets nothing.
    #[test]
    fn the_interpolation_fraction_is_guarded_and_clamped() {
        assert_eq!(
            interpolation_fraction(NetTick(10), NetTick(20), NetTick(15), 0.5),
            Some(0.55)
        );
        assert_eq!(
            interpolation_fraction(NetTick(10), NetTick(20), NetTick(5), 0.0),
            Some(0.0)
        );
        assert_eq!(
            interpolation_fraction(NetTick(10), NetTick(20), NetTick(25), 0.0),
            Some(1.0)
        );
        assert_eq!(
            interpolation_fraction(NetTick(10), NetTick(10), NetTick(10), 0.0),
            None
        );
        assert_eq!(
            interpolation_fraction(NetTick(20), NetTick(10), NetTick(15), 0.0),
            None
        );
    }
}
