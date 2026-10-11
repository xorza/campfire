use std::net::SocketAddr;
use std::time::Instant;

use bevy_ecs::entity::Entity;
use bytes::Bytes;
use quinn_proto::{
    ClientConfig, ConnectError, Connection, ConnectionError, ConnectionHandle, DatagramEvent,
    Endpoint, Event, Incoming,
};

use crate::app_code::AppCode;
use crate::outbox::Outbox;
use crate::recv_batch::Datagram;

/// The name every client sends: its verifier reads the key alone (design 15, D7).
const SERVER_NAME: &str = "campfire";

/// An endpoint and its connections, which its thread and the app's frame share behind one mutex
/// (design 15, D3, D9). A connection lives in a table by `quinn-proto`'s handle, with the link
/// entity that carries it, if any.
#[derive(Debug)]
pub(crate) struct EndpointState {
    endpoint: Endpoint,
    peers: Vec<Option<Peer>>,
    news: Vec<News>,
    next_id: u64,
    /// Whether a server's endpoint takes new connections: from its `LinkStart` until its
    /// `Unlink`, so a server that has not started or that ended refuses them.
    accepting: bool,
    closing: bool,
    max_segments: usize,
    /// Where `quinn-proto` writes one transmit, before the outbox keeps it.
    scratch: Vec<u8>,
    #[cfg(test)]
    retries: usize,
}

/// A connection, its slot, its id, which no later connection in its slot shares, and its link
/// entity.
#[derive(Debug)]
pub(crate) struct Peer {
    pub(crate) connection: Connection,
    handle: ConnectionHandle,
    id: u64,
    entity: Option<Entity>,
    linked: bool,
}

/// What the thread tells the frame: a handshake completed, with the connection's largest datagram
/// then, before path MTU discovery sent its first probe; or a link's connection was lost.
#[derive(Debug)]
pub(crate) enum News {
    Connected {
        handle: ConnectionHandle,
        id: u64,
        max_size: Option<usize>,
    },
    Lost {
        entity: Entity,
        reason: ConnectionError,
    },
}

impl EndpointState {
    pub(crate) const fn new(endpoint: Endpoint, max_segments: usize) -> EndpointState {
        EndpointState {
            endpoint,
            peers: Vec::new(),
            news: Vec::new(),
            next_id: 0,
            accepting: false,
            closing: false,
            max_segments,
            scratch: Vec::new(),
            #[cfg(test)]
            retries: 0,
        }
    }

    pub(crate) const fn closing(&self) -> bool {
        self.closing
    }

    pub(crate) const fn accept(&mut self, accepting: bool) {
        self.accepting = accepting;
    }

    /// The earliest deadline of the connections; none when no connection waits on one.
    pub(crate) fn next_deadline(&mut self) -> Option<Instant> {
        self.peers
            .iter_mut()
            .flatten()
            .filter_map(|peer| peer.connection.poll_timeout())
            .min()
    }

    /// Gives a datagram of the socket to the endpoint, and what it makes of it to its connection.
    pub(crate) fn take(&mut self, now: Instant, datagram: Datagram, outbox: &mut Outbox) {
        self.scratch.clear();
        match self.endpoint.handle(
            now,
            datagram.remote,
            datagram.local_ip,
            datagram.ecn,
            datagram.data,
            &mut self.scratch,
        ) {
            Some(DatagramEvent::ConnectionEvent(handle, event)) => {
                if let Some(peer) = self.peer_at(handle) {
                    peer.connection.handle_event(event);
                }
            }
            Some(DatagramEvent::NewConnection(incoming)) => self.admit(now, incoming, outbox),
            Some(DatagramEvent::Response(transmit)) => outbox.push(&transmit, &self.scratch),
            None => {}
        }
    }

    /// Answers a new connection: a Retry for an address that has not proved itself, so a spoofed
    /// one costs no state, and the connection for one whose Retry token holds (D8).
    fn admit(&mut self, now: Instant, incoming: Incoming, outbox: &mut Outbox) {
        self.scratch.clear();
        if self.closing || !self.accepting {
            let transmit = self.endpoint.refuse(incoming, &mut self.scratch);
            outbox.push(&transmit, &self.scratch);
            return;
        }
        if !incoming.remote_address_validated() {
            match self.endpoint.retry(incoming, &mut self.scratch) {
                Ok(transmit) => {
                    outbox.push(&transmit, &self.scratch);
                    #[cfg(test)]
                    {
                        self.retries += 1;
                    }
                }
                Err(error) => self.endpoint.ignore(error.into_incoming()),
            }
            return;
        }
        match self.endpoint.accept(incoming, now, &mut self.scratch, None) {
            Ok((handle, connection)) => {
                self.insert(handle, connection, None);
            }
            Err(error) => {
                if let Some(transmit) = error.response {
                    outbox.push(&transmit, &self.scratch);
                }
            }
        }
    }

    /// Opens a connection to `server`, which the link entity `entity` carries.
    pub(crate) fn connect(
        &mut self,
        now: Instant,
        config: ClientConfig,
        server: SocketAddr,
        entity: Entity,
    ) -> Result<(ConnectionHandle, u64), ConnectError> {
        let (handle, connection) = self.endpoint.connect(now, config, server, SERVER_NAME)?;
        Ok((handle, self.insert(handle, connection, Some(entity))))
    }

    fn insert(
        &mut self,
        handle: ConnectionHandle,
        connection: Connection,
        entity: Option<Entity>,
    ) -> u64 {
        if self.peers.len() <= handle.0 {
            self.peers.resize_with(handle.0 + 1, || None);
        }
        debug_assert!(
            self.peers[handle.0].is_none(),
            "quinn-proto gives a free handle"
        );
        let id = self.next_id;
        self.next_id += 1;
        self.peers[handle.0] = Some(Peer {
            connection,
            handle,
            id,
            entity,
            linked: false,
        });
        id
    }

    /// One pass of the thread over each connection: its due deadline, its events for the
    /// endpoint and from it, its news for the frame, and what it has to send. A drained
    /// connection leaves the table.
    pub(crate) fn serve(&mut self, now: Instant, outbox: &mut Outbox) {
        let EndpointState {
            endpoint,
            peers,
            news,
            max_segments,
            scratch,
            ..
        } = self;
        for (index, slot) in peers.iter_mut().enumerate() {
            let Some(peer) = slot else {
                continue;
            };
            let handle = ConnectionHandle(index);
            let connection = &mut peer.connection;
            if connection
                .poll_timeout()
                .is_some_and(|deadline| deadline <= now)
            {
                connection.handle_timeout(now);
            }
            let mut drained = false;
            while let Some(event) = connection.poll_endpoint_events() {
                drained |= event.is_drained();
                if let Some(event) = endpoint.handle_event(handle, event) {
                    connection.handle_event(event);
                }
            }
            while let Some(event) = connection.poll() {
                match event {
                    Event::Connected => news.push(News::Connected {
                        handle,
                        id: peer.id,
                        max_size: connection.datagrams().max_size(),
                    }),
                    Event::ConnectionLost { reason } => {
                        if let Some(entity) = peer.entity.take() {
                            news.push(News::Lost { entity, reason });
                        }
                    }
                    Event::HandshakeDataReady
                    | Event::Stream(_)
                    | Event::DatagramReceived
                    | Event::DatagramsUnblocked => {}
                }
            }
            loop {
                scratch.clear();
                let Some(transmit) = connection.poll_transmit(now, *max_segments, scratch) else {
                    break;
                };
                outbox.push(&transmit, scratch);
            }
            if drained {
                *slot = None;
            }
        }
    }

    /// Hands the news since the last frame to `into`, which the frame clears.
    pub(crate) fn take_news(&mut self, into: &mut Vec<News>) {
        into.append(&mut self.news);
    }

    /// The connection `id` in the slot `handle`, while it lives.
    pub(crate) fn peer(&mut self, handle: ConnectionHandle, id: u64) -> Option<&mut Peer> {
        self.peer_at(handle).filter(|peer| peer.id == id)
    }

    fn peer_at(&mut self, handle: ConnectionHandle) -> Option<&mut Peer> {
        self.peers.get_mut(handle.0).and_then(Option::as_mut)
    }

    /// The connections that carry a linked entity, with it.
    pub(crate) fn linked(&mut self) -> impl Iterator<Item = (Entity, &mut Peer)> {
        self.peers
            .iter_mut()
            .flatten()
            .filter(|peer| peer.linked)
            .filter_map(|peer| peer.entity.map(|entity| (entity, peer)))
    }

    /// Closes every connection with `AppCode::STOPPED`; the thread writes the closes, then ends.
    pub(crate) fn close_all(&mut self, now: Instant) {
        self.closing = true;
        for peer in self.peers.iter_mut().flatten() {
            peer.entity = None;
            peer.connection
                .close(now, AppCode::STOPPED.varint(), Bytes::new());
        }
    }
}

impl Peer {
    /// The connection's handshake completed and its entity `entity` carries it.
    pub(crate) const fn link(&mut self, entity: Entity) {
        self.entity = Some(entity);
        self.linked = true;
    }

    pub(crate) const fn entity(&self) -> Option<Entity> {
        self.entity
    }

    pub(crate) const fn handle(&self) -> ConnectionHandle {
        self.handle
    }

    pub(crate) const fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn remote(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    /// Closes the connection with `code` if the entity `entity` still carries it, and parts it
    /// from its entity.
    pub(crate) fn close(&mut self, now: Instant, entity: Entity, code: AppCode) {
        if self.entity != Some(entity) {
            return;
        }
        self.entity = None;
        self.connection.close(now, code.varint(), Bytes::new());
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use super::*;

    impl EndpointState {
        pub(crate) const fn retries(&self) -> usize {
            self.retries
        }

        /// Closes each connection no entity carries yet, as one lost between its handshake and
        /// the frame that would link it.
        pub(crate) fn close_unlinked(&mut self) {
            for peer in self.peers.iter_mut().flatten() {
                if peer.entity.is_none() {
                    peer.connection
                        .close(Instant::now(), AppCode::ENDED.varint(), Bytes::new());
                }
            }
        }

        /// Drops the connection the entity `entity` carries with no close, as a peer that stops
        /// sends none; the endpoint keeps nothing of it.
        pub(crate) fn forget(&mut self, entity: Entity) {
            for slot in &mut self.peers {
                if slot
                    .as_ref()
                    .is_some_and(|peer| peer.entity == Some(entity))
                {
                    *slot = None;
                }
            }
        }
    }
}
