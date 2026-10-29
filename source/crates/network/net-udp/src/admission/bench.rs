//! `atomic/udp/admission/…`: a server's admission of a run of handshakes, a flood and rejoins.

use campfire_net_core::Instant;
use core::time::Duration;
use std::hint::black_box;
use std::net::{IpAddr, Ipv4Addr};
use std::num::NonZeroU32;

use campfire_measure::{INPUTS, Kind, Scale};
use criterion::{BatchSize, Criterion};

use crate::admission::Admission;
use crate::admission_setup::AdmissionSetup;

/// The addresses of `rejoins`.
const PLAYERS: u32 = 64;

/// A server's endpoint's admission of `INPUTS` handshakes, each past its Retry, from a new
/// `Admission` of 8 for each address over 60 s: `flood`, from as many addresses at one instant, as
/// many real addresses send them, each admitted and its bucket kept; `rejoins`, from `PLAYERS`
/// addresses, one each 10 ms, most refused once their buckets run dry.
pub(crate) fn admission(c: &mut Criterion) {
    let setup = AdmissionSetup::new(
        NonZeroU32::new(8).unwrap(),
        NonZeroU32::MAX,
        Duration::from_secs(60),
    );
    let start = Instant::ZERO;
    let address = |nth: u32| {
        let [_, a, b, c] = nth.to_be_bytes();
        IpAddr::V4(Ipv4Addr::new(100, a, b, c.wrapping_add(1)))
    };
    let inputs = u32::try_from(INPUTS).expect("a bench's inputs fit u32");
    let flood: Vec<(IpAddr, Instant)> = (0..inputs).map(|nth| (address(nth), start)).collect();
    let rejoins: Vec<(IpAddr, Instant)> = (0..inputs)
        .map(|nth| {
            let at = start + Duration::from_millis(10 * u64::from(nth));
            (address(nth % PLAYERS), at)
        })
        .collect();
    let mut group = Kind::Atomic.group(c, "admission", Scale::Sample);
    group.throughput(campfire_measure::elements(INPUTS));
    for (name, handshakes) in [("flood", &flood), ("rejoins", &rejoins)] {
        group.bench_function(name, |b| {
            b.iter_batched_ref(
                || Admission::new(setup, start),
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
