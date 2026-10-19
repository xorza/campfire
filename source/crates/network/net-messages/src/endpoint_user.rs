use campfire_net_core::LinkDirection;

/// Who uses a sender or receiver of an entity: its link, or a host client's own client side.
///
/// A host client is one entity that is both the client and the server's link to it, so it keeps
/// an endpoint for each side; every other link has only its link's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndpointUser {
    /// The link's own endpoint: on a host client, the server's link to it.
    Link,
    /// A host client's own endpoint, which its client systems use.
    HostClient,
}

impl EndpointUser {
    /// The direction an endpoint of this user sends in: a remote client's link toward its server,
    /// every other link toward its client, a host client's included, and a host client's own side
    /// toward its server.
    pub(crate) const fn sends(self, remote_client: bool) -> LinkDirection {
        match self {
            Self::Link if remote_client => LinkDirection::ClientToServer,
            Self::Link => LinkDirection::ServerToClient,
            Self::HostClient => LinkDirection::ClientToServer,
        }
    }

    /// The user a host client's message that travels in `direction` lands on: the other one.
    pub(crate) const fn receiving(direction: LinkDirection) -> Self {
        match direction {
            LinkDirection::ServerToClient => Self::HostClient,
            LinkDirection::ClientToServer => Self::Link,
        }
    }
}
