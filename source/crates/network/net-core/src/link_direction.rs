/// One way a packet or message travels between a client and its server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkDirection {
    ClientToServer,
    ServerToClient,
}

impl LinkDirection {
    /// The position of this direction in a table that holds one entry for each direction.
    pub const fn index(self) -> usize {
        match self {
            Self::ClientToServer => 0,
            Self::ServerToClient => 1,
        }
    }

    const fn bit(self) -> u8 {
        1 << self.index()
    }
}

/// A set of [`LinkDirection`]s, as a registration declares the ways a channel or message travels.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkDirections(u8);

impl LinkDirections {
    /// Both directions.
    pub const BOTH: Self =
        Self(LinkDirection::ClientToServer.bit() | LinkDirection::ServerToClient.bit());

    pub const fn contains(self, direction: LinkDirection) -> bool {
        self.0 & direction.bit() != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The directions in `self` or in `other`.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl From<LinkDirection> for LinkDirections {
    fn from(direction: LinkDirection) -> Self {
        Self(direction.bit())
    }
}

/// The set as one bit for each direction, as a protocol hash holds it.
impl From<LinkDirections> for u8 {
    fn from(directions: LinkDirections) -> Self {
        directions.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each direction holds its own bit: one direction contains only itself, `BOTH` is the union of
    /// the two, and the empty set contains neither.
    #[test]
    fn a_set_contains_exactly_its_directions() {
        use LinkDirection::{ClientToServer, ServerToClient};
        let to_server = LinkDirections::from(ClientToServer);
        let to_client = LinkDirections::from(ServerToClient);
        assert!(to_server.contains(ClientToServer));
        assert!(!to_server.contains(ServerToClient));
        assert!(to_client.contains(ServerToClient));
        assert!(!to_client.contains(ClientToServer));
        assert_eq!(to_server.union(to_client), LinkDirections::BOTH);
        assert_eq!(to_server.union(to_server), to_server);
        assert!(
            LinkDirections::BOTH.contains(ClientToServer)
                && LinkDirections::BOTH.contains(ServerToClient)
        );
        assert!(LinkDirections::default().is_empty());
        assert!(!LinkDirections::default().contains(ClientToServer));
        assert!(!LinkDirections::default().contains(ServerToClient));
        assert!(!to_server.is_empty());
        assert_eq!([ClientToServer.index(), ServerToClient.index()], [0, 1]);
    }
}
