use campfire_common::SegmentSeed;
use campfire_math::{Num, Rng, RngSource, RngStream, Vec3};

/// A made scene of units for a bench or a test, drawn from a seed, the same in every run: the
/// kernel cases of every capability draw their units from one.
#[derive(Debug)]
pub(crate) struct KernelScene {
    rng: Rng,
}

impl KernelScene {
    /// The units of a kernel case: the count design 04 sets for an RTS battle.
    pub(crate) const UNITS: usize = 1000;
    /// A kernel case's scenes, each by its name and half its side in meters: crowded into 40 m
    /// square, and spread over 120 m square.
    pub(crate) const CROWDED: (&str, u64) = ("crowded", 20);
    pub(crate) const SPREAD: (&str, u64) = ("spread", 60);
    /// Both scenes, for a kernel whose cost grows with how close its units stand.
    pub(crate) const DENSITIES: [(&str, u64); 2] = [KernelScene::CROWDED, KernelScene::SPREAD];

    pub(crate) fn new(seed: u64) -> KernelScene {
        let source = RngSource::new(SegmentSeed::new([0; 32]));
        KernelScene {
            rng: source.open(RngStream::new("scene"), seed),
        }
    }

    /// A point on the ground at a whole centimeter within `span` meters of the origin on both
    /// axes.
    pub(crate) fn point(&mut self, span: u64) -> Vec3 {
        let mut coordinate = || {
            let cm = self.rng.below(span * 200).cast_signed();
            KernelScene::centimeters(cm - span.cast_signed() * 100)
        };
        let x = coordinate();
        Vec3::new(x, Num::ZERO, coordinate())
    }

    /// A draw below `bound`.
    pub(crate) fn below(&mut self, bound: u64) -> u64 {
        self.rng.below(bound)
    }

    /// `cm` centimeters.
    pub(crate) const fn centimeters(cm: i64) -> Num {
        Num::from_bits((cm << Num::FRAC_BITS) / 100)
    }
}
