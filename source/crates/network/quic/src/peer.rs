use std::net::SocketAddr;
use std::time::Instant;

use bevy_ecs::entity::Entity;
use bytes::Bytes;
use campfire_common::ChannelBinding;
use quinn_proto::{Connection, ConnectionHandle};

use crate::LINK_MIN_MTU;
use crate::app_code::AppCode;
use crate::link_binding::LinkBinding;

/// A connection in its endpoint's table, and the link entity that carries it (15-quic-transport.md,
/// A link's entity is its peer, not its address).
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
    pub(crate) const fn new(connection: Connection, key: PeerKey, client: Option<Entity>) -> Self {
        let carrier = match client {
            Some(entity) => Carrier::Linking(entity),
            None => Carrier::None,
        };
        Self {
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

    /// The connection's largest datagram now, while it carries `LINK_MIN_MTU`: a Campfire peer's
    /// always does, and only a peer that gives longer connection ids or a smaller
    /// `max_datagram_frame_size` makes it less (15-quic-transport.md, A link's minimum MTU is
    /// QUIC's datagram).
    pub(crate) fn link_mtu(&mut self) -> Option<usize> {
        self.connection
            .datagrams()
            .max_size()
            .filter(|&max| max >= LINK_MIN_MTU)
    }

    /// The connection's `tls-exporter` channel binding of RFC 9266, which a completed handshake
    /// gives both ends alike (D11).
    pub(crate) fn binding(&self) -> LinkBinding {
        let mut bytes = [0; 32];
        self.connection
            .crypto_session()
            .export_keying_material(&mut bytes, b"EXPORTER-Channel-Binding", b"")
            .expect("TLS 1.3's exporter gives 32 bytes");
        LinkBinding(ChannelBinding::new(bytes))
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
