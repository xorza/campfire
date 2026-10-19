use campfire_net_core::Direction;

/// Which side of its entity a sender or receiver serves.
///
/// A host client is one entity that is both the client and the server's link to it, so it keeps
/// an endpoint for each side; every other link has only its link side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndpointSide {
    /// The link's own endpoint: on a host client, the server's link to it.
    Link,
    /// A host client's own endpoint, which its client systems use.
    HostClient,
}

impl EndpointSide {
    /// The direction an endpoint of this side sends in: a remote client's link toward its server,
    /// every other link toward its client, a host client's included, and a host client's own side
    /// toward its server.
    pub(crate) const fn sends(self, remote_client: bool) -> Direction {
        match self {
            Self::Link if remote_client => Direction::ClientToServer,
            Self::Link => Direction::ServerToClient,
            Self::HostClient => Direction::ClientToServer,
        }
    }

    /// The side a host client's message that travels in `direction` lands on: the other one.
    pub(crate) const fn receiving(direction: Direction) -> Self {
        match direction {
            Direction::ServerToClient => Self::HostClient,
            Direction::ClientToServer => Self::Link,
        }
    }
}
