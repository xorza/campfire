use std::collections::BTreeSet;

use campfire_net_core::Instant;

/// The tokens for a next connection a server took, each until it expires, so each validates one
/// address once and a party that saw it pass cannot use it again (11-net-stack/udp.md, A server
/// checks an address before it keeps state). It holds no token past its expiry, when the token's
/// own check refuses it.
#[derive(Debug, Default)]
pub(crate) struct UsedTokens {
    /// The expiry first, so the oldest come first.
    used: BTreeSet<(Instant, u64)>,
}

impl UsedTokens {
    /// Takes the token of `nonce` that expires at `expires`, at `now`: whether no one took it
    /// before. The tokens that expired by `now` go first.
    pub(crate) fn take(&mut self, nonce: u64, expires: Instant, now: Instant) -> bool {
        while self.used.first().is_some_and(|&(oldest, _)| oldest <= now) {
            self.used.pop_first();
        }
        self.used.insert((expires, nonce))
    }
}

#[cfg(test)]
mod tests;
