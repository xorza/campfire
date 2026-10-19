use crate::value32::{Meaning, Value32};

/// The Ed25519 public key a server presents to TLS with no certificate (11-quic-transport.md, A raw
/// key derived from the server's key): the client takes this key and no other, in hex as a listing
/// gives it.
pub type TlsKey = Value32<TlsKeyMeaning>;

/// The meaning of a `TlsKey`.
#[derive(Debug)]
pub enum TlsKeyMeaning {}

impl Meaning for TlsKeyMeaning {
    const NAME: &'static str = "TlsKey";
}
