//! A moment on an app's network clock.

use core::ops::{Add, AddAssign, Sub};
use core::time::Duration;

#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;

/// A moment on an app's network clock: the time since the app's first update, in nanoseconds
/// (11-net-stack/time.md, The stack's own `Instant`, a `Duration` since the app's first update).
///
/// It counts from the origin of `Time<Real>::elapsed()`, so a driven clock reads the same values
/// on every run of a test, and two instants of one app subtract to the time between them. A `u64`
/// of nanoseconds spans 584 years at the precision of a `Duration`, in half its size.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
pub struct Instant(u64);

impl Instant {
    /// The app's first update.
    pub const ZERO: Self = Self(0);

    /// The moment `elapsed` after the app's first update.
    ///
    /// # Panics
    ///
    /// When `elapsed` is 584 years or more.
    pub const fn from_elapsed(elapsed: Duration) -> Self {
        Self(nanos(elapsed).expect("an app's clock reads less than 584 years"))
    }

    /// The time since the app's first update.
    pub const fn elapsed(self) -> Duration {
        Duration::from_nanos(self.0)
    }

    /// The time from `earlier` to `self`, or zero when `earlier` is later.
    pub const fn saturating_duration_since(self, earlier: Self) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(earlier.0))
    }

    /// The time from `earlier` to `self`, or `None` when `earlier` is later.
    pub const fn checked_duration_since(self, earlier: Self) -> Option<Duration> {
        match self.0.checked_sub(earlier.0) {
            Some(nanos) => Some(Duration::from_nanos(nanos)),
            None => None,
        }
    }

    /// The moment `duration` before `self`, or the first update when that is earlier.
    #[must_use]
    pub const fn saturating_sub(self, duration: Duration) -> Self {
        match nanos(duration) {
            Some(nanos) => Self(self.0.saturating_sub(nanos)),
            None => Self::ZERO,
        }
    }
}

/// `duration` in nanoseconds, or `None` when a `u64` cannot hold it.
const fn nanos(duration: Duration) -> Option<u64> {
    let nanos = duration.as_nanos();
    if nanos > u64::MAX as u128 {
        return None;
    }
    Some(nanos as u64)
}

/// The moment `duration` after `self`.
///
/// # Panics
///
/// When the moment is 584 years or more after the first update, as `std::time::Instant` panics
/// past its range.
impl Add<Duration> for Instant {
    type Output = Self;

    fn add(self, duration: Duration) -> Self {
        Self(
            nanos(duration)
                .and_then(|nanos| self.0.checked_add(nanos))
                .expect("overflow when adding a duration to an instant"),
        )
    }
}

impl AddAssign<Duration> for Instant {
    fn add_assign(&mut self, duration: Duration) {
        *self = *self + duration;
    }
}

/// The time between two moments, zero when `rhs` is later, as `std::time::Instant` does.
impl Sub for Instant {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Duration {
        self.saturating_duration_since(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_counts_from_the_first_update() {
        let start = Instant::from_elapsed(Duration::from_millis(250));
        let later = start + Duration::from_millis(40);
        assert_eq!(later.elapsed(), Duration::from_millis(290));
        assert_eq!(later - start, Duration::from_millis(40));
        // `rhs` later than `self` saturates to zero, as `std::time::Instant` does.
        assert_eq!(start - later, Duration::ZERO);
        assert_eq!(start.checked_duration_since(later), None);
        assert_eq!(
            start.checked_duration_since(Instant::ZERO),
            Some(Duration::from_millis(250))
        );
        assert_eq!(start.saturating_sub(Duration::from_secs(1)), Instant::ZERO);
        let mut moved = start;
        moved += Duration::from_millis(10);
        assert_eq!(moved, Instant::from_elapsed(Duration::from_millis(260)));
        // A nanosecond is kept, and a duration past what a `u64` holds saturates the subtraction.
        assert_eq!(
            (start + Duration::from_nanos(1)) - start,
            Duration::from_nanos(1)
        );
        assert_eq!(start.saturating_sub(Duration::MAX), Instant::ZERO);
        assert_eq!(size_of::<Instant>(), 8);
    }

    /// The last nanosecond a `u64` holds is a moment, and one more panics, as an add past it does.
    #[test]
    fn the_clock_spans_what_a_u64_of_nanoseconds_holds() {
        let last = Instant::from_elapsed(Duration::from_nanos(u64::MAX));
        assert_eq!(last.elapsed(), Duration::from_nanos(u64::MAX));
        assert!(
            std::panic::catch_unwind(|| Instant::from_elapsed(
                Duration::from_nanos(u64::MAX) + Duration::from_nanos(1)
            ))
            .is_err()
        );
        assert!(std::panic::catch_unwind(|| last + Duration::from_nanos(1)).is_err());
    }
}
