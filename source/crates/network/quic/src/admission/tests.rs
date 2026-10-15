use std::net::Ipv6Addr;
use std::num::NonZeroU32;
use std::time::Duration;

use super::*;

#[test]
fn an_address_takes_its_seats_of_handshakes_each_back_after_its_share_of_the_refill() {
    // Two seats over 60 s: each handshake comes back 30 s after the bucket was full again.
    let limit = AddressLimit::new(NonZeroU32::new(2).unwrap(), Duration::from_secs(60));
    let start = Instant::now();
    let mut admission = Admission::new(limit, start);
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
    let limit = AddressLimit::new(NonZeroU32::new(3).unwrap(), Duration::from_secs(60));
    let now = Instant::now();
    let mut admission = Admission::new(limit, now);
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7));
    // Three live connections of three seats refuse a fourth, with handshakes in the bucket;
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
