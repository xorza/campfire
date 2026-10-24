use std::hint::black_box;

use campfire_measure::{INPUTS, Kind, Scale};
use criterion::Criterion;

use crate::direction::Direction;
use crate::root::bench::{signed, values};

/// `Direction`'s operations over `INPUTS` inputs each, in three axes and, `_flat`, in two, as the
/// circles' queries and the boxes' take them, each axis of 1 to 46 bits, either sign, as offsets
/// within the world's bound are: `of`, a direction from a vector; `along`, an offset's distance
/// along one; `apart`, its distance from the line; and `scaled_away`, the vector along one of a
/// length of 1 to 37 bits, as a push takes it.
pub(crate) fn direction(c: &mut Criterion) {
    let axes = |seed: u64| signed(seed, 46);
    let (x, y, z) = (axes(30), axes(31), axes(32));
    let vectors: Vec<[i128; 3]> = (0..INPUTS).map(|at| [x[at], y[at], z[at]]).collect();
    let offs: Vec<[i128; 3]> = (0..INPUTS).map(|at| [z[at], x[at], y[at]]).collect();
    let flat_vectors: Vec<[i128; 2]> = vectors.iter().map(|&[x, _, z]| [x, z]).collect();
    let flat_offs: Vec<[i128; 2]> = offs.iter().map(|&[x, _, z]| [x, z]).collect();
    let lengths: Vec<i128> = values(33, 37)
        .into_iter()
        .map(|length| i128::try_from(length).expect("a length of at most 37 bits"))
        .collect();
    let directions: Vec<Direction<3>> = vectors
        .iter()
        .map(|&vector| Direction::of(vector).expect("an axis of at least one bit"))
        .collect();
    let flats: Vec<Direction<2>> = flat_vectors
        .iter()
        .map(|&vector| Direction::of(vector).expect("an axis of at least one bit"))
        .collect();

    let mut group = Kind::Atomic.group(c, "direction", Scale::Sample);
    group.throughput(campfire_measure::elements(INPUTS));
    group.bench_function("of", |bench| {
        bench.iter(|| {
            for &vector in &vectors {
                black_box(Direction::of(black_box(vector)));
            }
        });
    });
    group.bench_function("of_flat", |bench| {
        bench.iter(|| {
            for &vector in &flat_vectors {
                black_box(Direction::of(black_box(vector)));
            }
        });
    });
    group.bench_function("along", |bench| {
        bench.iter(|| {
            for (direction, &off) in directions.iter().zip(&offs) {
                black_box(black_box(direction).along(off));
            }
        });
    });
    group.bench_function("along_flat", |bench| {
        bench.iter(|| {
            for (direction, &off) in flats.iter().zip(&flat_offs) {
                black_box(black_box(direction).along(off));
            }
        });
    });
    group.bench_function("apart", |bench| {
        bench.iter(|| {
            for (direction, &off) in directions.iter().zip(&offs) {
                black_box(black_box(direction).apart(off));
            }
        });
    });
    group.bench_function("apart_flat", |bench| {
        bench.iter(|| {
            for (direction, &off) in flats.iter().zip(&flat_offs) {
                black_box(black_box(direction).apart(off));
            }
        });
    });
    group.bench_function("scaled_away", |bench| {
        bench.iter(|| {
            for (direction, &length) in directions.iter().zip(&lengths) {
                black_box(black_box(direction).scaled_away(length));
            }
        });
    });
    group.bench_function("scaled_away_flat", |bench| {
        bench.iter(|| {
            for (direction, &length) in flats.iter().zip(&lengths) {
                black_box(black_box(direction).scaled_away(length));
            }
        });
    });
    group.finish();
}
