use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::net::{IpAddr, Ipv4Addr};

use campfire_net_core::Instant;

use crate::admission_setup::AdmissionSetup;

/// What a server's endpoint admits from each address while it takes connections
/// (11-net-stack/udp.md, Each address holds a bounded count of connections and handshakes): a
/// connection whose address holds fewer live connections than `per_address`, and has a handshake
/// left in its bucket, and in its IPv6 /48's, which holds `PER_SITE` times as many. A bucket holds
/// its handshakes and gets one back each refill's share, as the generic cell rate algorithm keeps
/// a token bucket in one number: the time it is full again. A handshake finds its bucket in a
/// search of a tree and the full buckets go by their times, so a flood from many addresses costs
/// each handshake a logarithm of them; the tree keeps that bound for keys an attacker picks, which
/// a hashed map with a fixed hasher does not.
#[derive(Debug)]
pub(crate) struct Admission {
    setup: AdmissionSetup,
    /// When the admission started, from which each bucket's time counts.
    start: Instant,
    /// The bucket of each IPv4 address and IPv6 /64.
    addresses: Buckets,
    /// The bucket of each IPv6 /48, a site's delegation of 65,536 /64s.
    sites: Buckets,
}

/// The handshakes an IPv6 /48 holds for each its /64 holds, so a site's 65,536 /64s take no more
/// of a server than 16 of its hosts do (11-net-stack/udp.md, Each address holds a bounded count of
/// connections and handshakes).
pub(crate) const PER_SITE: u32 = 16;

/// Token buckets of `per` handshakes each over one refill, by key.
#[derive(Debug)]
struct Buckets {
    per: u32,
    /// Each key whose bucket is not full, with the time it is full again: nanoseconds since the
    /// admission's start times `per`, so a handshake's share of the refill adds exactly.
    full_at: BTreeMap<u128, u128>,
    /// Each time a bucket took, the earliest first: one whose bucket holds a later time since goes,
    /// with nothing else, as its time comes. A key holds at most `per` of them, as its bucket
    /// holds `per` handshakes.
    times: BinaryHeap<Reverse<(u128, u128)>>,
}

impl Buckets {
    const fn new(per: u32) -> Self {
        Self {
            per,
            full_at: BTreeMap::new(),
            times: BinaryHeap::new(),
        }
    }

    /// `elapsed` nanoseconds in the buckets' scale.
    fn scaled(&self, elapsed: u128) -> u128 {
        elapsed * u128::from(self.per)
    }

    /// Forgets each bucket full again by `now`, in the buckets' scale.
    fn forget(&mut self, now: u128) {
        while let Some(&Reverse((full_at, key))) = self.times.peek()
            && full_at <= now
        {
            self.times.pop();
            if self.full_at.get(&key) == Some(&full_at) {
                self.full_at.remove(&key);
            }
        }
    }

    /// When `key`'s bucket is full again, `now` for a full one.
    fn full_at(&self, key: u128, now: u128) -> u128 {
        self.full_at.get(&key).copied().unwrap_or(now)
    }

    /// Whether `key`'s bucket holds a handshake at `now`, of a refill of `share` nanoseconds.
    fn has_room(&self, key: u128, now: u128, share: u128) -> bool {
        self.full_at(key, now) - now <= share * u128::from(self.per - 1)
    }

    /// Takes a handshake of `key`'s bucket at `now`.
    fn take(&mut self, key: u128, now: u128, share: u128) {
        let full_at = self.full_at(key, now) + share;
        self.full_at.insert(key, full_at);
        self.times.push(Reverse((full_at, key)));
    }
}

/// An address as the limit counts it: an IPv4 address, or an IPv6 address's /64, which one host
/// often holds whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

    /// The bits of its IPv6 /48; none for an IPv4 address.
    fn site(self) -> Option<u128> {
        match self {
            Self::V4(_) => None,
            Self::V6(prefix) => Some(u128::from(u64::from_be_bytes(prefix) >> 16)),
        }
    }
}

impl Admission {
    pub(crate) const fn new(setup: AdmissionSetup, now: Instant) -> Self {
        let per = setup.per_address().get();
        Self {
            setup,
            start: now,
            addresses: Buckets::new(per),
            sites: Buckets::new(per.saturating_mul(PER_SITE)),
        }
    }

    /// Whether a connection from `ip` may start its handshake at `now`, where `live` counts the
    /// live connections of an address: one from a loopback address may; one from an address that
    /// holds `per_address` live connections, or whose bucket or site's bucket is empty, may not;
    /// any other takes a handshake of both.
    pub(crate) fn admit(
        &mut self,
        ip: IpAddr,
        now: Instant,
        live: impl FnOnce(AddressKey) -> usize,
    ) -> bool {
        let Some(key) = AddressKey::of(ip) else {
            return true;
        };
        let share = self.setup.refill().as_nanos();
        let elapsed = now.saturating_duration_since(self.start).as_nanos();
        let (address_now, site_now) = (self.addresses.scaled(elapsed), self.sites.scaled(elapsed));
        self.addresses.forget(address_now);
        self.sites.forget(site_now);
        let full = u32::try_from(live(key)).map_or(true, |live| live >= self.addresses.per);
        let site = key.site();
        let room = self.addresses.has_room(key.bits(), address_now, share)
            && site.is_none_or(|site| self.sites.has_room(site, site_now, share));
        if full || !room {
            return false;
        }
        self.addresses.take(key.bits(), address_now, share);
        if let Some(site) = site {
            self.sites.take(site, site_now, share);
        }
        true
    }
}

#[cfg(feature = "internals")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
