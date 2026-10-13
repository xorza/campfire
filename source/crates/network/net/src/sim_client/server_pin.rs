use std::num::NonZeroU32;

use campfire_common::TlsKey;
use secp256k1::XOnlyPublicKey;

/// The server a player means to reach, as its listing names it: its key, which the offered terms
/// must name, its TLS key, which the transport takes and no other, and its tick rate, which the offered terms must fix. The client's link ticks at that rate from
/// its start, before any offer comes, as Lightyear reads its tick length once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerPin {
    pub key: XOnlyPublicKey,
    pub tls_key: TlsKey,
    pub tick_hz: NonZeroU32,
}
