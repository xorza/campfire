use campfire_net_core::LinkDirection;

/// The end of an `EmulatedIo` link an entity holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Client,
    Server,
}

impl Side {
    pub(crate) const fn sends(self) -> LinkDirection {
        match self {
            Self::Client => LinkDirection::ClientToServer,
            Self::Server => LinkDirection::ServerToClient,
        }
    }

    pub(crate) const fn receives(self) -> LinkDirection {
        match self {
            Self::Client => LinkDirection::ServerToClient,
            Self::Server => LinkDirection::ClientToServer,
        }
    }

    /// The other end of the link.
    pub(crate) const fn peer(self) -> Self {
        match self {
            Self::Client => Self::Server,
            Self::Server => Self::Client,
        }
    }

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Client => 0,
            Self::Server => 1,
        }
    }
}
