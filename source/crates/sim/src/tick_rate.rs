use std::num::NonZeroU32;

use bevy_ecs::resource::Resource;

use crate::tick::Ticks;

/// Ticks a second, fixed for the whole match by the session's terms. Data gives times in
/// milliseconds and rates per second; they become ticks with it.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickRate(NonZeroU32);

impl TickRate {
    pub const fn new(hz: NonZeroU32) -> TickRate {
        TickRate(hz)
    }

    pub const fn hz(self) -> NonZeroU32 {
        self.0
    }

    /// `ms` milliseconds in ticks, rounded up, so nothing happens early; `None` when it does not
    /// fit a `u64`.
    pub const fn ticks(self, ms: u64) -> Option<Ticks> {
        match ms.checked_mul(self.0.get() as u64) {
            Some(scaled) => Some(Ticks::new(scaled.div_ceil(1000))),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milliseconds_round_up_to_whole_ticks() {
        // At 30 Hz 250 ms is 7.5 ticks, up to 8; at 20 Hz exactly 5; at 60 Hz exactly 15. 1 ms
        // is under a tick at every rate, up to 1; 2000 ms is exactly 40, 60 and 120.
        let cases = [
            (20, [(0, 0), (1, 1), (250, 5), (2000, 40)]),
            (30, [(0, 0), (1, 1), (250, 8), (2000, 60)]),
            (60, [(0, 0), (1, 1), (250, 15), (2000, 120)]),
        ];
        for (hz, at_rate) in cases {
            let rate = TickRate::new(NonZeroU32::new(hz).unwrap());
            for (ms, ticks) in at_rate {
                assert_eq!(
                    rate.ticks(ms),
                    Some(Ticks::new(ticks)),
                    "{ms} ms at {hz} Hz"
                );
            }
            assert_eq!(rate.ticks(u64::MAX), None);
        }
    }
}
