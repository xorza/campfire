use std::hint::black_box;

use campfire_measure::{INPUTS, Kind, Scale};
use criterion::Criterion;

use crate::root::bench::values;
use crate::rounding::Rounding;

/// The modes the inputs take in turn.
const MODES: [Rounding; 3] = [Rounding::NearestEven, Rounding::Floor, Rounding::Ceiling];

/// `Rounding`'s operations over `INPUTS` inputs each, the three modes in turn: `divide` of a
/// value of 1 to 120 bits, of either sign, by one of 1 to 64 bits, as `Num`'s division and its
/// scaled products take it; and `shift_right` of such a value by 1 to 120 bits, as `Num`'s
/// product takes it.
pub(crate) fn rounding(c: &mut Criterion) {
    let signed = |seed: u64, widest: u32| -> Vec<i128> {
        let drawn = values(seed, widest);
        let sign = |at: usize| if at.is_multiple_of(2) { 1 } else { -1 };
        let signed = drawn.iter().enumerate();
        signed
            .map(|(at, &value)| value.cast_signed() * sign(at))
            .collect()
    };
    let (numerators, denominators) = (signed(14, 120), signed(15, 64));
    let shifts: Vec<u32> = (0..INPUTS)
        .map(|at| u32::try_from(at % 120).expect("a shift below 120") + 1)
        .collect();

    let mut group = Kind::Atomic.group(c, "rounding", Scale::Sample);
    group.throughput(campfire_measure::elements(INPUTS));
    group.bench_function("divide", |bench| {
        bench.iter(|| {
            for (at, (&numerator, &denominator)) in numerators.iter().zip(&denominators).enumerate()
            {
                let mode = MODES[at % MODES.len()];
                black_box(mode.divide(black_box(numerator), denominator));
            }
        });
    });
    group.bench_function("shift_right", |bench| {
        bench.iter(|| {
            for (at, (&value, &shift)) in numerators.iter().zip(&shifts).enumerate() {
                let mode = MODES[at % MODES.len()];
                black_box(mode.shift_right(black_box(value), shift));
            }
        });
    });
    group.finish();
}
