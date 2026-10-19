use campfire_common::EnumAll;

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use campfire_math::TickRate;

/// The pause and the speed of a local match, which its player sets and its client and its local
/// server share: each applies them to its own clock every frame. Neither enters the log.
#[derive(Debug)]
pub struct Pace {
    paused: AtomicBool,
    /// The speed's place in `PaceSpeed::ALL`.
    speed: AtomicU8,
}

/// A local match's game speed: its timelines' rate is the session's times the speed, and the
/// sim's rate stays the session's (12-lightyear-in-tree.md, The pace sets the tick rate).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, EnumAll)]
pub enum PaceSpeed {
    Half,
    #[default]
    Normal,
    Double,
    Quadruple,
}

impl Default for Pace {
    fn default() -> Self {
        Self {
            paused: AtomicBool::new(false),
            speed: AtomicU8::new(PaceSpeed::Normal as u8),
        }
    }
}

impl Pace {
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn speed(&self) -> PaceSpeed {
        PaceSpeed::ALL[usize::from(self.speed.load(Ordering::Relaxed))]
    }

    pub fn set_speed(&self, speed: PaceSpeed) {
        self.speed.store(speed as u8, Ordering::Relaxed);
    }
}

impl PaceSpeed {
    /// The rate at this speed of a session whose rate is `rate`, exact: half of 21 Hz is 21 ticks
    /// every 2 s.
    pub const fn rate(self, rate: TickRate) -> TickRate {
        let (ticks, seconds) = match self {
            Self::Half => (1, 2),
            Self::Normal => (1, 1),
            Self::Double => (2, 1),
            Self::Quadruple => (4, 1),
        };
        TickRate::new(times(rate.ticks(), ticks), times(rate.seconds(), seconds))
    }
}

/// `count` times a speed's factor `by`.
const fn times(count: NonZeroU32, by: u32) -> NonZeroU32 {
    match count.checked_mul(NonZeroU32::new(by).expect("a speed's factor is not 0")) {
        Some(product) => product,
        None => panic!("a session's rate at a speed fits u32"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pace_holds_what_it_was_set_to_and_each_speed_scales_the_rate() {
        let pace = Pace::default();
        assert_eq!((pace.is_paused(), pace.speed()), (false, PaceSpeed::Normal));
        pace.set_paused(true);
        for speed in PaceSpeed::ALL {
            pace.set_speed(speed);
            assert_eq!(pace.speed(), speed);
        }
        assert!(pace.is_paused());
        // 21 Hz: 21 ticks every 2 s, 21, 42 and 84 Hz.
        let rate = |ticks, seconds| {
            TickRate::new(
                NonZeroU32::new(ticks).unwrap(),
                NonZeroU32::new(seconds).unwrap(),
            )
        };
        assert_eq!(
            PaceSpeed::ALL.map(|speed| speed.rate(rate(21, 1))),
            [rate(21, 2), rate(21, 1), rate(42, 1), rate(84, 1)]
        );
    }
}
