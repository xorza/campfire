use crate::bytes32::Bytes32;

/// What binds a connect answer to one TLS session: RFC 9266's `tls-exporter` value, 32 bytes of
/// TLS's exporter with the label `EXPORTER-Channel-Binding` and an empty context, the same at both
/// ends of one session and different for any other (design 15, D11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelBinding(Bytes32);

impl ChannelBinding {
    pub const fn new(bytes: [u8; 32]) -> ChannelBinding {
        ChannelBinding(Bytes32::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}
