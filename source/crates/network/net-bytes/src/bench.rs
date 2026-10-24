//! The crate's criterion cases, which `benches/net-bytes.rs` runs.
//!
//! `atomic/var_int/…`: `INPUTS` varints written to a buffer, and read back, a quarter of
//! them in each encoded length.

use campfire_measure::{INPUTS, Kind, Scale};
use criterion::{BatchSize, Criterion};
use std::hint::black_box;

use crate::reader::Reader;
use crate::var_int::VarInt;
use crate::writer::{WriteInteger, Writer};

/// The largest value of each varint length: 1, 2, 4 and 8 bytes.
const LARGEST: [u64; 4] = [63, 16_383, 1_073_741_823, VarInt::MAX.get()];

/// Runs each case of the crate whose id criterion's filter takes.
pub fn run(criterion: &mut Criterion) {
    campfire_measure::without_logs(criterion, varint);
}

fn varint(criterion: &mut Criterion) {
    // Each length's values step down from its largest by at most 63, so they keep its length.
    let inputs = u64::try_from(INPUTS).expect("a bench's inputs fit u64");
    let values: Vec<u64> = (0..inputs)
        .map(|i| LARGEST[(i % 4) as usize] - i / 4 % 64)
        .collect();
    let bytes: usize = values
        .iter()
        .map(|&value| VarInt::new(value).encoded_len())
        .sum();
    let mut encoded = Writer::with_capacity(bytes);
    for &value in &values {
        encoded.write_varint(value);
    }
    let encoded = encoded.into_bytes();

    let mut group = Kind::Atomic.group(criterion, "var_int", Scale::Sample);
    group.throughput(campfire_measure::elements(INPUTS));
    group.bench_function("write", |bencher| {
        let mut writer = Writer::with_capacity(bytes);
        bencher.iter(|| {
            writer.reset();
            for &value in &values {
                writer.write_varint(black_box(value));
            }
            debug_assert_eq!(writer.len(), bytes);
        });
    });
    group.bench_function("read", |bencher| {
        bencher.iter_batched_ref(
            || Reader::from(encoded.clone()),
            |reader| {
                for &value in &values {
                    let read = reader.read_varint().unwrap();
                    debug_assert_eq!(read, value);
                    black_box(read);
                }
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}
