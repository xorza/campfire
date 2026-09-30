use bevy_ecs::resource::Resource;

/// Ticks a second, fixed for the whole match. Data gives times in milliseconds and rates per
/// second; they become ticks with it.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickRate(u32);

impl TickRate {
    /// `None` for 0 ticks a second.
    pub const fn new(hz: u32) -> Option<TickRate> {
        if hz == 0 {
            return None;
        }
        Some(TickRate(hz))
    }

    pub const fn hz(self) -> u32 {
        self.0
    }

    /// `ms` milliseconds in ticks, rounded up, so nothing happens early; `None` when it does not
    /// fit a `u64`.
    pub const fn ticks(self, ms: u64) -> Option<u64> {
        match ms.checked_mul(self.0 as u64) {
            Some(scaled) => Some(scaled.div_ceil(1000)),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milliseconds_round_up_to_whole_ticks() {
        let rate = TickRate::new(30).unwrap();
        // 250 ms × 30 / 1000 = 7.5 ticks, up to 8; 2000 ms is exactly 60; 1 ms is 0.03, up to 1.
        let cases = [(0, Some(0)), (1, Some(1)), (250, Some(8)), (2000, Some(60))];
        for (ms, ticks) in cases {
            assert_eq!(rate.ticks(ms), ticks, "{ms} ms");
        }
        assert_eq!(rate.ticks(u64::MAX), None);
        assert_eq!(TickRate::new(0), None);
    }
}
