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

#[cfg(feature = "internals")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
