use std::net::{IpAddr, Ipv4Addr};
use std::time::Instant;

use crate::address_limit::AddressLimit;

/// What a server's endpoint admits from each address while it takes connections (design 15,
/// D13): a connection whose address holds fewer live connections than the seats, and has a
/// handshake left in its bucket. A bucket holds `seats` handshakes and gets one back each
/// `refill / seats`, as the generic cell rate algorithm keeps a token bucket in one number: the
/// time it is full again.
#[derive(Debug)]
pub(crate) struct Admission {
    limit: AddressLimit,
    /// When the limit started, from which each bucket's time counts.
    start: Instant,
    /// Each address whose bucket is not full, with the time it is full again: nanoseconds since
    /// `start` times the seats, so a handshake's share of the refill adds exactly.
    buckets: Vec<(AddressKey, u128)>,
}

/// An address as the limit counts it: an IPv4 address, or an IPv6 address's /64, which one host
/// often holds whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddressKey {
    V4(Ipv4Addr),
    V6([u8; 8]),
}

impl AddressKey {
    /// The key of `ip`, an IPv4 address mapped into IPv6 as the address it maps; none for a
    /// loopback address, whose peer runs on the server's own host, which no limit guards.
    pub(crate) const fn of(ip: IpAddr) -> Option<AddressKey> {
        let ip = ip.to_canonical();
        if ip.is_loopback() {
            return None;
        }
        Some(match ip {
            IpAddr::V4(ip) => AddressKey::V4(ip),
            IpAddr::V6(ip) => {
                let octets = ip.octets();
                AddressKey::V6(*octets.first_chunk().expect("an IPv6 address holds 8 bytes"))
            }
        })
    }
}

impl Admission {
    pub(crate) fn new(limit: AddressLimit, now: Instant) -> Admission {
        Admission {
            limit,
            start: now,
            buckets: Vec::with_capacity(usize::try_from(limit.seats().get()).unwrap_or(0)),
        }
    }

    /// Whether a connection from `ip` may start its handshake at `now`, where `live` counts the
    /// live connections of an address: one from a loopback address may; one from an address
    /// that holds `seats` live connections, or whose bucket is empty, may not; any other takes a
    /// handshake of its bucket.
    pub(crate) fn admit(
        &mut self,
        ip: IpAddr,
        now: Instant,
        live: impl FnOnce(AddressKey) -> usize,
    ) -> bool {
        let Some(key) = AddressKey::of(ip) else {
            return true;
        };
        let seats = self.limit.seats().get();
        let share = self.limit.refill().as_nanos();
        let now = now.saturating_duration_since(self.start).as_nanos() * u128::from(seats);
        self.buckets.retain(|&(_, full_at)| full_at > now);
        let at = self.buckets.iter().position(|&(held, _)| held == key);
        let full_at = at.map_or(now, |at| self.buckets[at].1);
        let spent = full_at - now;
        let room = share * u128::from(seats - 1);
        let seated = u32::try_from(live(key)).map_or(true, |live| live >= seats);
        if spent > room || seated {
            return false;
        }
        let full_at = full_at + share;
        match at {
            Some(at) => self.buckets[at].1 = full_at,
            None => self.buckets.push((key, full_at)),
        }
        true
    }
}

#[cfg(test)]
mod tests {
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
}
