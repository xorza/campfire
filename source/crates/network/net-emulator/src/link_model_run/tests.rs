use super::*;
use crate::rate_limit::RateLimit;

/// The delay after which each of packets `0..16`, all sent at once on a run seeded with `seed`,
/// is delivered; `None` marks a lost packet.
fn deliveries(model: &LinkModel, seed: u64) -> [Option<Duration>; 16] {
    let mut run = LinkModelRun::new(seed);
    [(); 16].map(|()| {
        run.admit(model, 100, Instant::ZERO)
            .map(|delivery| delivery.at.elapsed())
    })
}

#[test]
fn the_seed_alone_decides_losses_and_jitter() {
    let model = LinkModel::default()
        .with_latency(Duration::from_millis(100))
        .with_jitter(Duration::from_millis(20))
        .with_fixed_loss(Num::HALF);
    // From a separate model of the same draws: per packet, one `chance` for the state
    // transition (probability 0), one `chance` of 1/2 for the loss, and for a delivered packet
    // `100 ms + below(40 ms) - 20 ms`, to the nanosecond.
    let ns = |nanos| Some(Duration::from_nanos(nanos));
    assert_eq!(
        deliveries(&model, 1),
        [
            ns(118_840_110),
            None,
            ns(100_922_687),
            ns(96_165_686),
            None,
            None,
            ns(112_614_023),
            ns(82_638_407),
            None,
            None,
            ns(108_550_832),
            ns(103_914_086),
            None,
            None,
            ns(109_929_413),
            ns(114_473_131),
        ]
    );
    assert_eq!(
        deliveries(&model, 2)[..4],
        [ns(103_825_523), None, ns(109_563_492), ns(93_579_265)]
    );
}

#[test]
fn a_perfect_link_delivers_at_once() {
    let now = Instant::from_elapsed(Duration::from_millis(5));
    let mut run = LinkModelRun::new(0);
    assert_eq!(
        run.admit(&LinkModel::perfect(), 1200, now),
        Some(Arrival {
            at: now,
            copy_at: None
        })
    );
}

#[test]
fn the_rate_limit_queues_then_drops_at_the_tail() {
    // 1000 B/s with a 2000 B queue, three 1000 B packets at once: the first leaves after 1 s,
    // the second waits for it (1000 B queued, room for 1000 more) and leaves at 2 s, and the
    // third finds 2000 B queued and is dropped.
    let model = LinkModel::perfect().with_rate(RateLimit::new(1000, 2000));
    let mut run = LinkModelRun::new(0);
    let at = |delivery: Option<Arrival>| delivery.map(|delivery| delivery.at.elapsed());
    assert_eq!(
        at(run.admit(&model, 1000, Instant::ZERO)),
        Some(Duration::from_secs(1))
    );
    assert_eq!(
        at(run.admit(&model, 1000, Instant::ZERO)),
        Some(Duration::from_secs(2))
    );
    assert_eq!(at(run.admit(&model, 1000, Instant::ZERO)), None);
    // At 1.5 s the queue holds the half-sent second packet, 500 B: a 1000 B packet fits and
    // leaves when the second is done, at 2 s, plus its own 1 s.
    let later = Instant::from_elapsed(Duration::from_millis(1500));
    assert_eq!(
        at(run.admit(&model, 1000, later)),
        Some(Duration::from_secs(3))
    );
}

#[test]
fn a_reordered_packet_skips_the_latency_and_a_duplicate_has_its_own_delivery() {
    let delayed = LinkModel::perfect().with_latency(Duration::from_millis(100));
    let mut run = LinkModelRun::new(0);
    assert_eq!(
        run.admit(&delayed, 100, Instant::ZERO)
            .map(|delivery| delivery.at.elapsed()),
        Some(Duration::from_millis(100))
    );
    let reordered = delayed.clone().with_reorder(Num::ONE);
    assert_eq!(
        run.admit(&reordered, 100, Instant::ZERO)
            .map(|delivery| delivery.at),
        Some(Instant::ZERO)
    );
    let duplicated = delayed.with_duplicate(Num::ONE);
    assert_eq!(
        run.admit(&duplicated, 100, Instant::ZERO)
            .and_then(|delivery| delivery.copy_at)
            .map(Instant::elapsed),
        Some(Duration::from_millis(100))
    );
}

#[test]
fn a_duplicate_pays_its_bytes_behind_the_original() {
    // 1000 B/s with a 1500 B queue and 100 ms latency, a 1000 B packet always duplicated: the
    // original leaves at 1 s and arrives at 1.1 s; the copy finds 1000 B queued, so 2000 B do not
    // fit and it is never made.
    let model = LinkModel::perfect()
        .with_latency(Duration::from_millis(100))
        .with_rate(RateLimit::new(1000, 1500))
        .with_duplicate(Num::ONE);
    let mut run = LinkModelRun::new(0);
    let delivery = run.admit(&model, 1000, Instant::ZERO).unwrap();
    assert_eq!(delivery.at.elapsed(), Duration::from_millis(1100));
    assert_eq!(delivery.copy_at, None);

    // With room for both, 500 B each: the original leaves at 0.5 s, the copy behind it at 1 s.
    let mut run = LinkModelRun::new(0);
    let delivery = run.admit(&model, 500, Instant::ZERO).unwrap();
    assert_eq!(delivery.at.elapsed(), Duration::from_millis(600));
    assert_eq!(
        delivery.copy_at.map(Instant::elapsed),
        Some(Duration::from_millis(1100))
    );
}

#[test]
fn lifting_the_rate_limit_keeps_the_queue_in_order() {
    // A 1000 B packet on 1000 B/s leaves at 1 s; a packet sent right after the limit is lifted
    // waits for it, as a real queue drains in order.
    let limited = LinkModel::perfect().with_rate(RateLimit::new(1000, 2000));
    let mut run = LinkModelRun::new(0);
    let at = |delivery: Option<Arrival>| delivery.map(|delivery| delivery.at.elapsed());
    assert_eq!(
        at(run.admit(&limited, 1000, Instant::ZERO)),
        Some(Duration::from_secs(1))
    );
    let lifted = Instant::from_elapsed(Duration::from_millis(100));
    assert_eq!(
        at(run.admit(&LinkModel::perfect(), 1000, lifted)),
        Some(Duration::from_secs(1))
    );
}
