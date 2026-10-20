//! The tokens that prove a client holds its address: a Retry's, for one handshake, and the ones a
//! server gives in a handshake for the client's next connection, each usable once
//! (11-net-stack/udp.md, A server checks an address before it keeps state).

use campfire_net_core::Instant;
use core::net::IpAddr;
use core::time::Duration;
use ring::hmac;

/// How long a Retry's token lasts: the round trip of the client's next Initial.
pub(crate) const RETRY_LIFETIME: Duration = Duration::from_secs(10);

/// How long a token for the next connection lasts: two minutes, the time a client tries to rejoin.
pub(crate) const NEXT_LIFETIME: Duration = Duration::from_secs(120);

/// The bytes of a token: its kind, its expiry, its nonce, and the first 16 bytes of its HMAC.
pub(crate) const TOKEN_LEN: usize = 1 + 8 + 8 + 16;

/// What a token proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// A Retry's: the address of one handshake, which its ID names.
    Retry,
    /// A handshake's gift: the address, for one later connection.
    Next,
}

impl TokenKind {
    const fn byte(self) -> u8 {
        match self {
            Self::Retry => 0,
            Self::Next => 1,
        }
    }
}

/// A token the key made and whose HMAC holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CheckedToken {
    pub(crate) kind: TokenKind,
    pub(crate) expires: Instant,
    pub(crate) nonce: u64,
}

/// The key of a server's address tokens: random at each start, so a token outlives no server.
#[derive(Debug)]
pub(crate) struct TokenKey(hmac::Key);

impl TokenKey {
    pub(crate) fn new(secret: &[u8; 32]) -> Self {
        Self(hmac::Key::new(hmac::HMAC_SHA256, secret))
    }

    /// The token of `kind` for `ip`, which expires at `expires`: a Retry's names `handshake`, and
    /// a next connection's carries `nonce`, which the server's log takes once.
    pub(crate) fn mint(
        &self,
        kind: TokenKind,
        ip: IpAddr,
        handshake: [u8; 8],
        nonce: u64,
        expires: Instant,
    ) -> [u8; TOKEN_LEN] {
        let mut token = [0; TOKEN_LEN];
        token[0] = kind.byte();
        token[1..9].copy_from_slice(&nanos(expires).to_be_bytes());
        token[9..17].copy_from_slice(&nonce.to_be_bytes());
        let tag = self.tag(&token[..17], ip, handshake);
        token[17..].copy_from_slice(&tag);
        token
    }

    /// The token `token` for `ip` and the handshake `handshake`, when this key made it, it has not
    /// expired by `now`, and a Retry's names that handshake.
    pub(crate) fn check(
        &self,
        token: &[u8],
        ip: IpAddr,
        handshake: [u8; 8],
        now: Instant,
    ) -> Option<CheckedToken> {
        let token: &[u8; TOKEN_LEN] = token.try_into().ok()?;
        let kind = match token[0] {
            0 => TokenKind::Retry,
            1 => TokenKind::Next,
            _ => return None,
        };
        let named = match kind {
            TokenKind::Retry => handshake,
            TokenKind::Next => [0; 8],
        };
        let tag = self.tag(&token[..17], ip, named);
        let difference = tag
            .iter()
            .zip(&token[17..])
            .fold(0, |difference, (a, b)| difference | (a ^ b));
        if difference != 0 {
            return None;
        }
        let expires = Instant::from_elapsed(Duration::from_nanos(u64::from_be_bytes(
            token[1..9].try_into().expect("8 bytes"),
        )));
        (now < expires).then(|| CheckedToken {
            kind,
            expires,
            nonce: u64::from_be_bytes(token[9..17].try_into().expect("8 bytes")),
        })
    }

    fn tag(&self, body: &[u8], ip: IpAddr, handshake: [u8; 8]) -> [u8; 16] {
        let ip = match ip.to_canonical() {
            IpAddr::V4(ip) => ip.to_ipv6_mapped().octets(),
            IpAddr::V6(ip) => ip.octets(),
        };
        let mut context = hmac::Context::with_key(&self.0);
        context.update(body);
        context.update(&ip);
        context.update(&handshake);
        let mut tag = [0; 16];
        tag.copy_from_slice(&context.sign().as_ref()[..16]);
        tag
    }
}

fn nanos(instant: Instant) -> u64 {
    u64::try_from(instant.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::net::Ipv4Addr;

    fn at(secs: u64) -> Instant {
        Instant::from_elapsed(Duration::from_secs(secs))
    }

    /// A token holds for its address and, a Retry's, its handshake, until it expires: another IP,
    /// another handshake, another key, a changed byte, or its expiry refuses it. A next
    /// connection's token is not bound to a handshake.
    #[test]
    fn a_token_holds_for_its_address_and_handshake_until_it_expires() {
        let key = TokenKey::new(&[1; 32]);
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let other = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));
        let retry = key.mint(TokenKind::Retry, ip, [7; 8], 0, at(10));
        assert_eq!(
            key.check(&retry, ip, [7; 8], at(9)),
            Some(CheckedToken {
                kind: TokenKind::Retry,
                expires: at(10),
                nonce: 0
            })
        );
        assert_eq!(key.check(&retry, ip, [7; 8], at(10)), None);
        assert_eq!(key.check(&retry, other, [7; 8], at(9)), None);
        assert_eq!(key.check(&retry, ip, [8; 8], at(9)), None);
        assert_eq!(
            TokenKey::new(&[2; 32]).check(&retry, ip, [7; 8], at(9)),
            None
        );
        let mut changed = retry;
        changed[3] ^= 1;
        assert_eq!(key.check(&changed, ip, [7; 8], at(9)), None);
        let next = key.mint(TokenKind::Next, ip, [0; 8], 42, at(10));
        assert_eq!(
            key.check(&next, ip, [9; 8], at(9)).map(|token| token.nonce),
            Some(42)
        );
        assert_eq!(key.check(&next[..TOKEN_LEN - 1], ip, [9; 8], at(9)), None);
    }
}
