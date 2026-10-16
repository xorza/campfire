use std::cmp::Reverse;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BinaryHeap};
use std::net::{IpAddr, Ipv4Addr};
use std::time::Instant;

use crate::address_limit::AddressLimit;

/// What a server's endpoint admits from each address while it takes connections
/// (15-quic-transport.md, Limits for each address, from the session's seats): a connection whose
/// address holds fewer live connections than the seats, and has a handshake left in its bucket. A
/// bucket holds `seats` handshakes and gets one back each `refill / seats`, as the generic cell
/// rate algorithm keeps a token bucket in one number: the time it is full again. A handshake finds
/// its bucket in a search of a tree and the full buckets go by their times, so a flood from many
/// addresses costs each handshake a logarithm of them; the tree keeps that bound for keys an
/// attacker picks, which a hashed map with a fixed hasher does not.
#[derive(Debug)]
pub(crate) struct Admission {
    limit: AddressLimit,
    /// When the limit started, from which each bucket's time counts.
    start: Instant,
    /// Each address whose bucket is not full, with the time it is full again: nanoseconds since
    /// `start` times the seats, so a handshake's share of the refill adds exactly.
    buckets: BTreeMap<u128, u128>,
    /// Each time a bucket took, the earliest first: one whose bucket holds a later time since
    /// goes, with nothing else, as its time comes. An address holds at most `seats` of them, as
    /// its bucket holds `seats` handshakes.
    times: BinaryHeap<Reverse<(u128, u128)>>,
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
    pub(crate) const fn of(ip: IpAddr) -> Option<Self> {
        let ip = ip.to_canonical();
        if ip.is_loopback() {
            return None;
        }
        Some(match ip {
            IpAddr::V4(ip) => Self::V4(ip),
            IpAddr::V6(ip) => {
                let octets = ip.octets();
                Self::V6(*octets.first_chunk().expect("an IPv6 address holds 8 bytes"))
            }
        })
    }

    /// The key as one number, which the tree orders as fast as it compares: an IPv6 /64's bits,
    /// or an IPv4 address's past them.
    fn bits(self) -> u128 {
        match self {
            Self::V4(ip) => (1 << 64) | u128::from(ip.to_bits()),
            Self::V6(prefix) => u128::from(u64::from_be_bytes(prefix)),
        }
    }
}

impl Admission {
    pub(crate) const fn new(limit: AddressLimit, now: Instant) -> Self {
        Self {
            limit,
            start: now,
            buckets: BTreeMap::new(),
            times: BinaryHeap::new(),
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
        while let Some(&Reverse((full_at, held))) = self.times.peek()
            && full_at <= now
        {
            self.times.pop();
            if self.buckets.get(&held) == Some(&full_at) {
                self.buckets.remove(&held);
            }
        }
        let bits = key.bits();
        let bucket = self.buckets.entry(bits);
        let full_at = match &bucket {
            Entry::Occupied(held) => *held.get(),
            Entry::Vacant(_) => now,
        };
        let spent = full_at - now;
        let room = share * u128::from(seats - 1);
        let seated = u32::try_from(live(key)).map_or(true, |live| live >= seats);
        if spent > room || seated {
            return false;
        }
        let full_at = full_at + share;
        bucket.insert_entry(full_at);
        self.times.push(Reverse((full_at, bits)));
        true
    }
}

#[cfg(feature = "internals")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
