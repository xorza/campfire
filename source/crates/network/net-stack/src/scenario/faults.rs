use campfire_math::SplitMix64;
use campfire_net_core::Direction;
use campfire_net_emulator::{LinkModel, Phase, RateLimit};
use core::time::Duration;

/// The network faults of one scenario, each one on or off, as a swarm seed picks them.
#[derive(Debug, Clone, Default)]
pub(crate) struct Faults {
    pub(crate) latency: Option<Latency>,
    pub(crate) loss: Option<Loss>,
    pub(crate) duplicate: Option<f32>,
    pub(crate) reorder: Option<f32>,
    pub(crate) rate: Option<RateLimit>,
    pub(crate) outage: Option<Outage>,
}

/// The Simple Gilbert model's mean loss and mean burst length.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Loss {
    pub(crate) mean: f32,
    pub(crate) burst: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Latency {
    pub(crate) latency: Duration,
    pub(crate) jitter: Duration,
}

/// No packet passes `direction` (both when `None`) for `length` from `start` after the faults
/// begin.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Outage {
    pub(crate) start: Duration,
    pub(crate) length: Duration,
    pub(crate) direction: Option<Direction>,
}

impl Faults {
    /// Swarm testing (Groce et al., ISSTA 2012): each fault is on with probability one half, and
    /// its values are drawn too, so seeds explore runs with few faults as well as with many.
    pub(crate) fn from_seed(seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed);
        let flags = [(); 6].map(|()| rng.below(2) == 0);
        let latency = flags[0].then(|| Latency {
            latency: millis(&mut rng, 10, 150),
            jitter: millis(&mut rng, 0, 30),
        });
        let loss = flags[1].then(|| Loss {
            mean: upto(&mut rng, 0.1),
            burst: 1.0 + upto(&mut rng, 7.0),
        });
        let duplicate = flags[2].then(|| upto(&mut rng, 0.05));
        let reorder = flags[3].then(|| upto(&mut rng, 0.1));
        let rate = flags[4].then(|| {
            let bytes_per_second = 30_000 + u32::try_from(rng.below(170_000)).unwrap();
            RateLimit::new(bytes_per_second, bytes_per_second)
        });
        let outage = flags[5].then(|| Outage {
            start: millis(&mut rng, 200, 1200),
            length: millis(&mut rng, 100, 800),
            direction: match rng.below(3) {
                0 => Some(Direction::ClientToServer),
                1 => Some(Direction::ServerToClient),
                _ => None,
            },
        });
        Self {
            latency,
            loss,
            duplicate,
            reorder,
            rate,
            outage,
        }
    }

    /// The model both directions follow while the faults hold.
    pub(crate) fn model(&self) -> LinkModel {
        let mut model = LinkModel::perfect();
        if let Some(latency) = self.latency {
            model = model
                .with_latency(latency.latency)
                .with_jitter(latency.jitter);
        }
        if let Some(loss) = self.loss {
            model = model.with_simple_gilbert_loss(loss.mean, loss.burst);
        }
        if let Some(duplicate) = self.duplicate {
            model = model.with_duplicate(duplicate);
        }
        if let Some(reorder) = self.reorder {
            model = model.with_reorder(reorder);
        }
        if let Some(rate) = self.rate {
            model = model.with_rate(rate);
        }
        model
    }

    /// The phases of `direction` from `start`, the moment the faults begin, to `heal`.
    pub(crate) fn phases(
        &self,
        direction: Direction,
        start: Duration,
        heal: Duration,
    ) -> Vec<Phase> {
        let model = self.model();
        let mut phases = vec![Phase::recovery(start, model.clone())];
        if let Some(outage) = self.outage
            && outage.direction.is_none_or(|only| only == direction)
        {
            phases.push(Phase::outage(start + outage.start));
            phases.push(Phase::recovery(start + outage.start + outage.length, model));
        }
        phases.push(Phase::recovery(heal, LinkModel::perfect()));
        phases
    }
}

/// A value in `[0, max]`, in 10,000 even steps.
fn upto(rng: &mut SplitMix64, max: f32) -> f32 {
    rng.below(10_001) as f32 / 10_000.0 * max
}

fn millis(rng: &mut SplitMix64, low: u64, high: u64) -> Duration {
    Duration::from_millis(low + rng.below(high - low))
}
