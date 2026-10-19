use core::time::Duration;

/// How one app of the stepper runs: its frame interval, its clock drift and its stall.
#[derive(Debug, Clone)]
pub(crate) struct AppSchedule {
    pub(crate) frame_interval: Duration,
    /// How much faster the app's clock runs than the stepper's, in parts per million.
    drift_ppm: i32,
    /// The stepper time and the app's clock when the drift was last set, from which the clock
    /// follows in closed form.
    drift_since: Duration,
    local_at_drift: Duration,
    /// The app's own clock, which the stepper hands to its `TimeUpdateStrategy`.
    pub(crate) local: Duration,
    /// The stepper time of the app's next frame in `run_for`.
    pub(crate) next_frame: Duration,
    /// The stepper time before which the app runs no frame.
    pub(crate) stalled_until: Duration,
}

impl AppSchedule {
    /// An app whose clock reads `now` and whose first frame comes one interval later.
    pub(crate) fn new(frame_interval: Duration, now: Duration) -> Self {
        assert!(
            frame_interval > Duration::ZERO,
            "a frame interval is positive"
        );
        Self {
            frame_interval,
            drift_ppm: 0,
            drift_since: now,
            local_at_drift: now,
            local: now,
            next_frame: now + frame_interval,
            stalled_until: now,
        }
    }

    /// Records a frame of the app at stepper time `now`: its next one comes an interval later.
    pub(crate) fn ran_at(&mut self, now: Duration) {
        self.next_frame = now + self.frame_interval;
    }

    /// Runs the app's clock `drift_ppm` parts per million faster than the stepper's from stepper
    /// time `now`, or slower when negative.
    pub(crate) const fn set_drift(&mut self, drift_ppm: i32, now: Duration) {
        self.drift_ppm = drift_ppm;
        self.drift_since = now;
        self.local_at_drift = self.local;
    }

    /// Sets the app's clock for stepper time `now`, at its drift.
    ///
    /// The clock is computed from the time since the drift was set, in whole nanoseconds, so it
    /// reads the same however the stepper's time was split into steps: `elapsed` plus
    /// `elapsed * drift_ppm / 1_000_000`, rounded toward zero.
    pub(crate) fn advance_to(&mut self, now: Duration) {
        let nanos = i128::try_from((now - self.drift_since).as_nanos())
            .expect("a run fits in i128 nanoseconds");
        let drifted = nanos + nanos * i128::from(self.drift_ppm) / 1_000_000;
        let drifted = u64::try_from(drifted.max(0)).expect("a run fits in u64 nanoseconds");
        self.local = self.local_at_drift + Duration::from_nanos(drifted);
    }

    pub(crate) fn is_stalled(&self, now: Duration) -> bool {
        now < self.stalled_until
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1000 ppm of 1 s is 1 ms, and -500 ppm of 2 s is -1 ms.
    #[test]
    fn drift_adds_its_parts_per_million() {
        let mut schedule = AppSchedule::new(Duration::from_millis(16), Duration::ZERO);
        schedule.advance_to(Duration::from_secs(1));
        assert_eq!(schedule.local, Duration::from_secs(1));
        schedule.set_drift(1000, Duration::from_secs(1));
        schedule.advance_to(Duration::from_secs(2));
        assert_eq!(schedule.local, Duration::from_millis(2001));
        schedule.set_drift(-500, Duration::from_secs(2));
        schedule.advance_to(Duration::from_secs(4));
        assert_eq!(schedule.local, Duration::from_millis(4000));
    }

    /// 1 ppm drift over 1 s is 1 µs whether the second is one step or a thousand: rounding each
    /// 1 ms step toward zero would lose every nanosecond of it.
    #[test]
    fn the_clock_does_not_depend_on_how_time_is_stepped() {
        let mut whole = AppSchedule::new(Duration::from_millis(1), Duration::ZERO);
        whole.set_drift(1, Duration::ZERO);
        whole.advance_to(Duration::from_secs(1));
        let mut stepped = AppSchedule::new(Duration::from_millis(1), Duration::ZERO);
        stepped.set_drift(1, Duration::ZERO);
        for millis in 1..=1000 {
            stepped.advance_to(Duration::from_millis(millis));
        }
        assert_eq!(
            whole.local,
            Duration::from_secs(1) + Duration::from_micros(1)
        );
        assert_eq!(stepped.local, whole.local);
    }

    #[test]
    #[should_panic(expected = "a frame interval is positive")]
    fn a_zero_interval_is_refused() {
        AppSchedule::new(Duration::ZERO, Duration::ZERO);
    }
}
