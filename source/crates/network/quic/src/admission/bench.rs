use std::hint::black_box;
use std::net::{IpAddr, Ipv4Addr};
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use campfire_measure::{Kind, Scale};
use criterion::{BatchSize, Criterion, Throughput};

use crate::address_limit::AddressLimit;
use crate::admission::Admission;

/// The handshakes of each case, each iteration's.
const COUNT: u32 = 4096;

/// The addresses of `rejoins`.
const PLAYERS: u32 = 64;

/// A server's endpoint's admission of `COUNT` handshakes, each past its Retry, from a new
/// `Admission` of 8 seats over 60 s: `flood`, from as many addresses at one instant, as many real
/// addresses send them, each admitted and its bucket kept; `rejoins`, from `PLAYERS` addresses,
/// one each 10 ms, most refused once their buckets run dry.
pub(crate) fn admission(c: &mut Criterion) {
    let limit = AddressLimit::new(NonZeroU32::new(8).unwrap(), Duration::from_secs(60));
    let start = Instant::now();
    let address = |nth: u32| {
        let [_, a, b, c] = nth.to_be_bytes();
        IpAddr::V4(Ipv4Addr::new(100, a, b, c.wrapping_add(1)))
    };
    let flood: Vec<(IpAddr, Instant)> = (0..COUNT).map(|nth| (address(nth), start)).collect();
    let rejoins: Vec<(IpAddr, Instant)> = (0..COUNT)
        .map(|nth| {
            let at = start + Duration::from_millis(10 * u64::from(nth));
            (address(nth % PLAYERS), at)
        })
        .collect();
    let mut group = Kind::Atomic.group(c, "admission", Scale::Sample);
    group.throughput(Throughput::Elements(u64::from(COUNT)));
    for (name, handshakes) in [("flood", &flood), ("rejoins", &rejoins)] {
        group.bench_function(name, |b| {
            b.iter_batched_ref(
                || Admission::new(limit, start),
                |admission| {
                    for &(ip, now) in handshakes {
                        black_box(admission.admit(black_box(ip), now, |_| 0));
                    }
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}
