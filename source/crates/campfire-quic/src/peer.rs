use std::net::SocketAddr;
use std::time::Instant;

use bevy_ecs::entity::Entity;
use bytes::Bytes;
use quinn_proto::{Connection, ConnectionHandle};

use crate::app_code::AppCode;

/// A connection in its endpoint's table, and the link entity that carries it (design 15, D9).
#[derive(Debug)]
pub(crate) struct Peer {
    pub(crate) connection: Connection,
    key: PeerKey,
    carrier: Carrier,
}

/// A connection's slot in its endpoint's table, and its id, which no later connection in its slot
/// shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PeerKey {
    pub(crate) handle: ConnectionHandle,
    pub(crate) id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Carrier {
    /// A server's connection before its link spawns, or one closed or lost since.
    None,
    /// A client's entity, until its handshake completes.
    Linking(Entity),
    Linked(Entity),
}

impl Peer {
    /// A connection `key` names, which a client's entity `client` carries from its connect, and a
    /// server's link from its handshake's end.
    pub(crate) const fn new(connection: Connection, key: PeerKey, client: Option<Entity>) -> Peer {
        let carrier = match client {
            Some(entity) => Carrier::Linking(entity),
            None => Carrier::None,
        };
        Peer {
            connection,
            key,
            carrier,
        }
    }

    pub(crate) const fn key(&self) -> PeerKey {
        self.key
    }

    /// The entity that carries the connection, linked or linking.
    pub(crate) const fn entity(&self) -> Option<Entity> {
        match self.carrier {
            Carrier::None => None,
            Carrier::Linking(entity) | Carrier::Linked(entity) => Some(entity),
        }
    }

    /// The link entity of a connection whose handshake completed, while it carries it.
    pub(crate) const fn linked(&self) -> Option<Entity> {
        match self.carrier {
            Carrier::Linked(entity) => Some(entity),
            Carrier::None | Carrier::Linking(_) => None,
        }
    }

    pub(crate) fn remote(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    /// The connection's handshake completed and its entity `entity` carries it.
    pub(crate) const fn link(&mut self, entity: Entity) {
        self.carrier = Carrier::Linked(entity);
    }

    /// The connection was lost: it parts from its entity, which then has a loss to tell.
    pub(crate) const fn lose(&mut self) -> Option<Entity> {
        let entity = self.entity();
        self.carrier = Carrier::None;
        entity
    }

    /// Closes the connection with `code`, and parts it from its entity. `quinn-proto` ignores the
    /// close of a connection closed or lost already.
    pub(crate) fn close(&mut self, now: Instant, code: AppCode) {
        self.carrier = Carrier::None;
        self.connection.close(now, code.varint(), Bytes::new());
    }
}
