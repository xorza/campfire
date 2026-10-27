/// The time owed to the effects' steps: each frame adds its time, and takes the whole steps it
/// holds, so a system steps at the file's rate at any frame rate (02-engine-core.md, A system steps
/// as its file says).
#[derive(Debug, Default)]
pub(crate) struct StepClock {
    owed: f64,
}

impl StepClock {
    /// Adds `seconds` and takes the whole steps of `step` seconds owed.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a frame owes a count of steps of no sign far below 2³²"
    )]
    pub(crate) fn take(&mut self, seconds: f64, step: f32) -> u32 {
        let step = f64::from(step);
        self.owed += seconds;
        let steps = (self.owed / step).floor();
        self.owed -= steps * step;
        steps as u32
    }

    /// The share of a step of `step` seconds owed past the last whole step taken.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a share of a step from 0 to 1"
    )]
    pub(crate) fn fraction(&self, step: f32) -> f32 {
        (self.owed / f64::from(step)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_takes_the_whole_steps_its_time_and_the_time_owed_hold() {
        // Steps of 1/4 s: frames of 0.1 s owe 0.1, 0.2, then 0.3, one step and 0.05 kept, then
        // 0.15 and 0.25, another.
        let mut clock = StepClock::default();
        let steps: Vec<u32> = (0..5).map(|_| clock.take(0.1, 0.25)).collect();
        assert_eq!(steps, [0, 0, 1, 0, 1]);
        // A frame of 1.1 s takes 4 steps, and at steps of 1/8 s the same time takes 8.
        assert_eq!(StepClock::default().take(1.1, 0.25), 4);
        assert_eq!(StepClock::default().take(1.1, 0.125), 8);
        // After 1.1 s at steps of 1/4 s, 0.1 s is owed: 0.4 of a step.
        let mut clock = StepClock::default();
        clock.take(1.1, 0.25);
        assert!((clock.fraction(0.25) - 0.4).abs() < 1e-6);
    }
}
