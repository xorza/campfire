use campfire_net_core::NetId;

/// Which channels a flush sends from: every one, or one alone while the peers check their
/// protocol, the others' messages held whole until it passed (11-net-stack/transport.md, The
/// protocol check goes first on a reserved channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SendGate {
    #[default]
    Open,
    Only(NetId),
}

impl SendGate {
    /// Whether a flush sends from the channel `channel`.
    pub(crate) const fn sends(self, channel: NetId) -> bool {
        match self {
            Self::Open => true,
            Self::Only(only) => only.get() == channel.get(),
        }
    }
}
