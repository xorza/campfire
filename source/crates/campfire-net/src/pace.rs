use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;

/// The pause and the speed of a local match, which its player sets and its client and its local
/// server share: each applies them to its own clock every frame. Neither enters the log.
#[derive(Debug)]
pub struct Pace {
    paused: AtomicBool,
    /// The speed's place in `PaceSpeed::ALL`.
    speed: AtomicU8,
}

/// A local match's game speed: the tick length is the session's over the speed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PaceSpeed {
    Half,
    #[default]
    Normal,
    Double,
    Quadruple,
}

impl Default for Pace {
    fn default() -> Pace {
        Pace {
            paused: AtomicBool::new(false),
            speed: AtomicU8::new(PaceSpeed::Normal.place()),
        }
    }
}

impl Pace {
    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn speed(&self) -> PaceSpeed {
        PaceSpeed::ALL[usize::from(self.speed.load(Ordering::Relaxed))]
    }

    pub fn set_speed(&self, speed: PaceSpeed) {
        self.speed.store(speed.place(), Ordering::Relaxed);
    }
}

impl PaceSpeed {
    /// Every speed, slowest first.
    pub const ALL: [PaceSpeed; 4] = [
        PaceSpeed::Half,
        PaceSpeed::Normal,
        PaceSpeed::Double,
        PaceSpeed::Quadruple,
    ];

    /// The speed's place in `ALL`.
    const fn place(self) -> u8 {
        match self {
            PaceSpeed::Half => 0,
            PaceSpeed::Normal => 1,
            PaceSpeed::Double => 2,
            PaceSpeed::Quadruple => 3,
        }
    }

    /// The tick length at this speed of a session whose ticks last `tick`.
    pub const fn tick(self, tick: Duration) -> Duration {
        match self {
            PaceSpeed::Half => tick.saturating_mul(2),
            PaceSpeed::Normal => tick,
            PaceSpeed::Double => tick.checked_div(2).expect("2 is not 0"),
            PaceSpeed::Quadruple => tick.checked_div(4).expect("4 is not 0"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pace_holds_what_it_was_set_to_and_each_speed_divides_the_tick() {
        let pace = Pace::default();
        assert_eq!((pace.paused(), pace.speed()), (false, PaceSpeed::Normal));
        pace.set_paused(true);
        for speed in PaceSpeed::ALL {
            pace.set_speed(speed);
            assert_eq!(pace.speed(), speed);
        }
        assert!(pace.paused());
        // 50 ms ticks: 100, 50, 25 and 12.5 ms.
        let tick = Duration::from_millis(50);
        assert_eq!(
            PaceSpeed::ALL.map(|speed| speed.tick(tick)),
            [100_000, 50_000, 25_000, 12_500].map(Duration::from_micros)
        );
    }
}
