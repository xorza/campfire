/// A gateway's PCP or NAT-PMP reply that does not read: the reply it should be, by its version, its
/// opcode and its length, or a result code the protocol does not define.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ReplyError {
    #[error("a reply of {len} bytes, where it takes {needed}")]
    TooShort { len: usize, needed: usize },
    #[error("a reply of version {0}")]
    Version(u8),
    #[error("a reply with opcode {0}, which answers another request")]
    Opcode(u8),
    #[error("a reply with result code {0}, which the protocol does not define")]
    Result(u16),
    #[error("a reply that maps protocol {0}, not UDP")]
    Protocol(u8),
    #[error("a reply whose external address is no IPv4 address")]
    NotIpv4,
}
