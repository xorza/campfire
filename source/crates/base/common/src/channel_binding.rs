use crate::value32::{Meaning, Value32};

/// What binds a connect answer to one TLS session: RFC 9266's `tls-exporter` value, 32 bytes of
/// TLS's exporter with the label `EXPORTER-Channel-Binding` and an empty context, the same at both
/// ends of one session and different for any other (15-quic-transport.md, The answer binds to the
/// TLS session).
pub type ChannelBinding = Value32<ChannelBindingMeaning>;

/// The meaning of a `ChannelBinding`.
#[derive(Debug)]
pub enum ChannelBindingMeaning {}

impl Meaning for ChannelBindingMeaning {
    const NAME: &'static str = "ChannelBinding";
}
