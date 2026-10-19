//! The crate's criterion cases, which `benches/net-bytes.rs` runs.
//!
//! `atomic/varint/…`: `VALUES` varints written to a buffer, and read back, a quarter of
//! them in each encoded length.

use campfire_measure::Kind;
use campfire_measure::Scale;
use criterion::{BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;

use crate::reader::{ReadVarInt, Reader};
use crate::varint;
use crate::writer::{WriteInteger, Writer};

const VALUES: u64 = 1024;

/// The largest value of each varint length: 1, 2, 4 and 8 bytes.
const LARGEST: [u64; 4] = [63, 16_383, 1_073_741_823, varint::MAX];

/// Runs each case of the crate whose id criterion's filter takes.
pub fn run(criterion: &mut Criterion) {
    campfire_measure::without_logs(criterion, varint);
}

fn varint(criterion: &mut Criterion) {
    // Each length's values step down from its largest by at most 63, so they keep its length.
    let values: Vec<u64> = (0..VALUES)
        .map(|i| LARGEST[(i % 4) as usize] - i / 4 % 64)
        .collect();
    let bytes: usize = values.iter().map(|&value| varint::len(value)).sum();
    let mut encoded = Writer::with_capacity(bytes);
    for &value in &values {
        encoded.write_varint(value).unwrap();
    }
    let encoded = encoded.into_bytes();

    let mut group = Kind::Atomic.group(criterion, "varint", Scale::Sample);
    group.bench_function(BenchmarkId::from_parameter("write"), |bencher| {
        let mut writer = Writer::with_capacity(bytes);
        bencher.iter(|| {
            writer.reset();
            for &value in &values {
                writer.write_varint(black_box(value)).unwrap();
            }
            debug_assert_eq!(writer.len(), bytes);
        });
    });
    group.bench_function(BenchmarkId::from_parameter("read"), |bencher| {
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
