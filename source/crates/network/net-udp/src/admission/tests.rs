use std::net::Ipv6Addr;
use std::num::NonZeroU32;
use std::time::Duration;

use super::*;

#[test]
fn an_address_takes_its_handshakes_each_back_after_its_share_of_the_refill() {
    // Two for each address over 60 s: each handshake comes back 30 s after the bucket was full
    // again.
    let setup = AdmissionSetup::new(NonZeroU32::new(2).unwrap(), Duration::from_secs(60));
    let start = Instant::ZERO;
    let mut admission = Admission::new(setup, start);
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7));
    let at = |seconds: u64, nanos: u32| start + Duration::new(seconds, nanos);
    let mut admit = |now: Instant| admission.admit(ip, now, |_| 0);
    // Two at once, the bucket full; a third is refused.
    assert!(admit(start) && admit(start));
    assert!(!admit(start));
    // Full again at 60 s, one back at 30 s, and not a nanosecond before.
    assert!(!admit(at(29, 999_999_999)));
    assert!(admit(at(30, 0)));
    assert!(!admit(at(30, 0)));
    // At 60 s the bucket holds the one of 30 s's back, then another at 90 s.
    assert!(admit(at(60, 0)));
    assert!(!admit(at(60, 0)));
    assert!(admit(at(150, 0)) && admit(at(150, 0)) && !admit(at(150, 0)));
}

#[test]
fn the_live_connections_of_an_address_refuse_a_handshake_and_each_address_counts_alone() {
    let setup = AdmissionSetup::new(NonZeroU32::new(3).unwrap(), Duration::from_secs(60));
    let now = Instant::ZERO;
    let mut admission = Admission::new(setup, now);
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7));
    // Three live connections of three allowed refuse a fourth, with handshakes in the bucket;
    // two let it in.
    assert!(!admission.admit(ip, now, |_| 3));
    assert!(admission.admit(ip, now, |_| 2));
    // Its bucket spent, another address takes its own three.
    for _ in 0..2 {
        assert!(admission.admit(ip, now, |_| 0));
    }
    assert!(!admission.admit(ip, now, |_| 0));
    let other = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 8));
    for _ in 0..3 {
        assert!(admission.admit(other, now, |_| 0));
    }
    assert!(!admission.admit(other, now, |_| 0));
    // A loopback address is never limited, by its live connections or its handshakes.
    for loopback in [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ] {
        assert!(admission.admit(loopback, now, |_| 100));
    }
}

/// An IPv6 /48 holds `PER_SITE` times an address's handshakes: with one each, 16 of its /64s take
/// one each and a 17th is refused, while a /64 of another /48 takes its own.
#[test]
fn an_ipv6_site_holds_sixteen_addresses_worth_of_handshakes() {
    let setup = AdmissionSetup::new(NonZeroU32::MIN, Duration::from_secs(60));
    let now = Instant::ZERO;
    let mut admission = Admission::new(setup, now);
    let host =
        |site: u16, net: u16| IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, site, net, 0, 0, 0, 1));
    for net in 0..u16::try_from(PER_SITE).unwrap() {
        assert!(admission.admit(host(1, net), now, |_| 0), "{net}");
    }
    assert!(!admission.admit(host(1, 99), now, |_| 0));
    assert!(admission.admit(host(2, 0), now, |_| 0));
}

#[test]
fn an_ipv6_address_counts_by_its_64_and_a_mapped_ipv4_one_as_its_ipv4() {
    let v6 = |text: &str| AddressKey::of(text.parse().unwrap());
    assert_eq!(v6("2001:db8:1:2::1"), v6("2001:db8:1:2:ffff::9"));
    assert_ne!(v6("2001:db8:1:2::1"), v6("2001:db8:1:3::1"));
    assert_eq!(
        v6("::ffff:203.0.113.7"),
        Some(AddressKey::V4(Ipv4Addr::new(203, 0, 113, 7)))
    );
    assert_eq!(v6("::ffff:127.0.0.1"), None);
    assert_eq!(v6("::1"), None);
    assert_eq!(v6("127.8.0.1"), None);
}

/// The admission as a walk of every bucket, one each address, finds it: the cross-check of the
/// tree and its queue of times.
#[derive(Debug)]
struct EveryBucket {
    setup: AdmissionSetup,
    start: Instant,
    buckets: Vec<(AddressKey, u128)>,
}

impl EveryBucket {
    fn admit(&mut self, ip: IpAddr, now: Instant, live: usize) -> bool {
        let Some(key) = AddressKey::of(ip) else {
            return true;
        };
        let per_address = self.setup.per_address().get();
        let share = self.setup.refill().as_nanos();
        let now = now.saturating_duration_since(self.start).as_nanos() * u128::from(per_address);
        self.buckets.retain(|&(_, full_at)| full_at > now);
        let at = self.buckets.iter().position(|&(held, _)| held == key);
        let full_at = at.map_or(now, |at| self.buckets[at].1);
        if full_at - now > share * u128::from(per_address - 1)
            || live >= usize::try_from(per_address).unwrap()
        {
            return false;
        }
        match at {
            Some(at) => self.buckets[at].1 = full_at + share,
            None => self.buckets.push((key, full_at + share)),
        }
        true
    }
}

#[test]
fn a_handshake_is_admitted_as_a_walk_of_every_bucket_admits_it() {
    // Three for each address over 3 s, 20,000 handshakes from 40 addresses, a loopback one among
    // them, at steps of 0 to 99 ms, each with 0 to 3 live connections: each answer the walk's.
    let setup = AdmissionSetup::new(NonZeroU32::new(3).unwrap(), Duration::from_secs(3));
    let start = Instant::ZERO;
    let mut admission = Admission::new(setup, start);
    let mut every = EveryBucket {
        setup,
        start,
        buckets: Vec::new(),
    };
    let mut word: u64 = 0x5EED;
    let mut draw = |below: u64| {
        word = word
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (word >> 33) % below
    };
    let (mut now, mut admitted, mut refused) = (start, 0, 0);
    for _ in 0..20_000 {
        now += Duration::from_millis(draw(100));
        let nth = u8::try_from(draw(40)).unwrap();
        let ip = if nth == 0 {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            IpAddr::V4(Ipv4Addr::new(203, 0, 113, nth))
        };
        let live = usize::try_from(draw(4)).unwrap();
        let expected = every.admit(ip, now, live);
        assert_eq!(admission.admit(ip, now, |_| live), expected);
        if expected {
            admitted += 1;
        } else {
            refused += 1;
        }
    }
    // Both answers came often.
    assert!(admitted > 2_000 && refused > 2_000, "{admitted} {refused}");
}
