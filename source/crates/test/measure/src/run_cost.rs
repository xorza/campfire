use std::time::Duration;

/// What a run of a system's steps cost, ticks of a match or frames of a stepper, by the time each
/// step took: in all, its first, and the worst of the others, as the worst step is the budget
/// (02-engine-core.md, The worst tick is the budget).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunCost {
    pub total: Duration,
    pub first: Duration,
    pub worst: Duration,
}

impl RunCost {
    /// The cost of `steps` steps, each the time `step` gives.
    pub fn of(steps: u64, mut step: impl FnMut() -> Duration) -> Self {
        debug_assert!(steps > 0, "a run takes a step");
        let first = step();
        let mut cost = Self {
            total: first,
            first,
            worst: Duration::ZERO,
        };
        for _ in 1..steps {
            let spent = step();
            cost.total += spent;
            cost.worst = cost.worst.max(spent);
        }
        cost
    }

    /// The worst of all its steps, the first among them.
    pub fn worst_of_all(self) -> Duration {
        self.first.max(self.worst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_cost_splits_the_first_step_from_the_worst_of_the_rest() {
        // Step 0 takes 9 µs and step n after it n % 7 µs: the 5,999 steps after the first are
        // 857 whole rounds of 1 to 6 and 0, 21 µs each, so 17,997 µs, and 18,006 µs with the
        // first. The worst after the first is 6 µs; of all, the first's 9 µs.
        let mut n = 0;
        let cost = RunCost::of(6000, || {
            let micros = if n == 0 { 9 } else { n % 7 };
            n += 1;
            Duration::from_micros(micros)
        });
        assert_eq!(n, 6000);
        let micros = Duration::from_micros;
        assert_eq!(
            [cost.total, cost.first, cost.worst],
            [18_006, 9, 6].map(micros)
        );
        assert_eq!(cost.worst_of_all(), micros(9));
    }
}
