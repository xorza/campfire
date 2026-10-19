use campfire_net_core::Direction;

/// The end of a `EmulatedIo` link an entity holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Client,
    Server,
}

impl Side {
    pub(crate) const fn sends(self) -> Direction {
        match self {
            Self::Client => Direction::ClientToServer,
            Self::Server => Direction::ServerToClient,
        }
    }

    pub(crate) const fn receives(self) -> Direction {
        match self {
            Self::Client => Direction::ServerToClient,
            Self::Server => Direction::ClientToServer,
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
