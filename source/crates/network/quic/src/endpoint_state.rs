use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::Instant;

use bevy_ecs::entity::Entity;
use quinn_proto::{
    ClientConfig, ConnectError, Connection, ConnectionHandle, DatagramEvent, Endpoint, Event,
    Incoming,
};

use crate::address_limit::AddressLimit;
use crate::admission::{AddressKey, Admission};
use crate::app_code::AppCode;
use crate::link_end::LinkEnd;
use crate::outbox::Outbox;
use crate::peer::{Peer, PeerKey};
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
    news: VecDeque<News>,
    next_id: u64,
    /// What a server's endpoint admits of each address while it takes new connections: from its
    /// `LinkStart` until its `Unlink`, so a server that has not started or that ended refuses
    /// them all.
    admission: Option<Admission>,
    closing: bool,
    max_segments: usize,
    /// Where `quinn-proto` writes one transmit, before the outbox keeps it.
    scratch: Vec<u8>,
    #[cfg(test)]
    retries: usize,
}

/// What the thread tells the frame: a handshake completed on a connection whose datagrams carry a
/// link; or the connection of a linking or linked entity ended, as `end` says.
#[derive(Debug)]
pub(crate) enum News {
    Connected {
        key: PeerKey,
    },
    Lost {
        key: PeerKey,
        entity: Entity,
        end: LinkEnd,
    },
}

impl News {
    const fn key(&self) -> PeerKey {
        match self {
            Self::Connected { key } | Self::Lost { key, .. } => *key,
        }
    }
}

impl EndpointState {
    pub(crate) const fn new(endpoint: Endpoint, max_segments: usize) -> Self {
        Self {
            endpoint,
            peers: Vec::new(),
            news: VecDeque::new(),
            next_id: 0,
            admission: None,
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

    /// Takes new connections from `now`, within `limit` for each address.
    pub(crate) fn start_accepting(&mut self, limit: AddressLimit, now: Instant) {
        self.admission = Some(Admission::new(limit, now));
    }

    /// Takes no new connection, and closes with `AppCode::ENDED` each one no link carries yet, as
    /// a server that ended links none of them.
    pub(crate) fn stop_accepting(&mut self, now: Instant) {
        self.admission = None;
        for peer in self.peers.iter_mut().flatten() {
            if peer.entity().is_none() {
                peer.close(now, AppCode::ENDED);
            }
        }
    }

    /// The earliest deadline of the connections; none when no connection waits on one.
    pub(crate) fn next_deadline(&mut self) -> Option<Instant> {
        self.peers
            .iter_mut()
            .flatten()
            .filter_map(|peer| peer.connection.poll_timeout())
            .min()
    }

    /// The thread's work under the mutex for one read: each datagram to the endpoint, then each
    /// connection served (design 15, D3).
    pub(crate) fn pass(
        &mut self,
        now: Instant,
        datagrams: impl Iterator<Item = Datagram>,
        outbox: &mut Outbox,
    ) {
        for datagram in datagrams {
            self.take(now, datagram, outbox);
        }
        self.serve(now, outbox);
    }

    /// Gives a datagram of the socket to the endpoint, and what it makes of it to its connection.
    fn take(&mut self, now: Instant, datagram: Datagram, outbox: &mut Outbox) {
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
    /// one costs no state (D8); a refusal for one past its address's limit, before its handshake,
    /// so it costs no signature (D13); and the connection for any other.
    fn admit(&mut self, now: Instant, incoming: Incoming, outbox: &mut Outbox) {
        let Self {
            endpoint,
            peers,
            admission,
            scratch,
            ..
        } = self;
        scratch.clear();
        let Some(admission) = admission else {
            let transmit = endpoint.refuse(incoming, scratch);
            outbox.push(&transmit, scratch);
            return;
        };
        if !incoming.remote_address_validated() {
            match endpoint.retry(incoming, scratch) {
                Ok(transmit) => {
                    outbox.push(&transmit, scratch);
                    #[cfg(test)]
                    {
                        self.retries += 1;
                    }
                }
                Err(error) => endpoint.ignore(error.into_incoming()),
            }
            return;
        }
        let live = |key| {
            let live = peers.iter().flatten().map(|peer| &peer.connection);
            live.filter(|connection| {
                !connection.is_closed()
                    && AddressKey::of(connection.remote_address().ip()) == Some(key)
            })
            .count()
        };
        if !admission.admit(incoming.remote_address().ip(), now, live) {
            let transmit = endpoint.refuse(incoming, scratch);
            outbox.push(&transmit, scratch);
            return;
        }
        match endpoint.accept(incoming, now, scratch, None) {
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
    ) -> Result<PeerKey, ConnectError> {
        let (handle, connection) = self.endpoint.connect(now, config, server, SERVER_NAME)?;
        Ok(self.insert(handle, connection, Some(entity)))
    }

    fn insert(
        &mut self,
        handle: ConnectionHandle,
        connection: Connection,
        client: Option<Entity>,
    ) -> PeerKey {
        if self.peers.len() <= handle.0 {
            self.peers.resize_with(handle.0 + 1, || None);
        }
        debug_assert!(
            self.peers[handle.0].is_none(),
            "quinn-proto gives a free handle"
        );
        let key = PeerKey {
            handle,
            id: self.next_id,
        };
        self.next_id += 1;
        self.peers[handle.0] = Some(Peer::new(connection, key, client));
        key
    }

    /// One pass of the thread over each connection: its due deadline, its events for the
    /// endpoint and from it, its news for the frame, and what it has to send. A connection whose
    /// datagrams cannot carry `LINK_MIN_MTU` once its handshake completed is closed in the pass
    /// that finds it, so every connection the frame sees between two passes carries its link
    /// (D5). A drained connection leaves the table.
    fn serve(&mut self, now: Instant, outbox: &mut Outbox) {
        let Self {
            endpoint,
            peers,
            news,
            max_segments,
            scratch,
            ..
        } = self;
        for slot in peers.iter_mut() {
            let Some(peer) = slot else {
                continue;
            };
            let key = peer.key();
            if peer
                .connection
                .poll_timeout()
                .is_some_and(|deadline| deadline <= now)
            {
                peer.connection.handle_timeout(now);
            }
            let mut drained = false;
            while let Some(event) = peer.connection.poll_endpoint_events() {
                drained |= event.is_drained();
                if let Some(event) = endpoint.handle_event(key.handle, event) {
                    peer.connection.handle_event(event);
                }
            }
            while let Some(event) = peer.connection.poll() {
                match event {
                    Event::Connected => {
                        if peer.link_mtu().is_some() {
                            news.push_back(News::Connected { key });
                        }
                    }
                    Event::ConnectionLost { reason } => {
                        if let Some(entity) = peer.lose() {
                            news.push_back(News::Lost {
                                key,
                                entity,
                                end: LinkEnd::of(&reason),
                            });
                        }
                    }
                    Event::HandshakeDataReady
                    | Event::Stream(_)
                    | Event::DatagramReceived
                    | Event::DatagramsUnblocked => {}
                }
            }
            if !peer.connection.is_handshaking()
                && !peer.connection.is_closed()
                && peer.link_mtu().is_none()
            {
                if let Some(entity) = peer.entity() {
                    news.push_back(News::Lost {
                        key,
                        entity,
                        end: LinkEnd::Unfit,
                    });
                }
                peer.close(now, AppCode::UNFIT);
            }
            loop {
                scratch.clear();
                let Some(transmit) = peer.connection.poll_transmit(now, *max_segments, scratch)
                else {
                    break;
                };
                outbox.push(&transmit, scratch);
            }
            if drained {
                *slot = None;
            }
        }
    }

    /// The oldest news the frame has not taken.
    pub(crate) fn next_news(&mut self) -> Option<News> {
        self.news.pop_front()
    }

    /// The connection `key` names, while it lives.
    pub(crate) fn peer(&mut self, key: PeerKey) -> Option<&mut Peer> {
        self.peer_at(key.handle).filter(|peer| peer.key() == key)
    }

    fn peer_at(&mut self, handle: ConnectionHandle) -> Option<&mut Peer> {
        self.peers.get_mut(handle.0).and_then(Option::as_mut)
    }

    /// Closes the connection `key` names with `code`, while it lives, and drops its news: a link
    /// its own end closed has no loss to tell, which would end the entity's next connection.
    pub(crate) fn close(&mut self, key: PeerKey, now: Instant, code: AppCode) {
        if let Some(peer) = self.peer(key) {
            peer.close(now, code);
        }
        self.news.retain(|news| news.key() != key);
    }

    /// The connections that carry a linked entity, with it.
    pub(crate) fn linked(&mut self) -> impl Iterator<Item = (Entity, &mut Peer)> {
        self.peers
            .iter_mut()
            .flatten()
            .filter_map(|peer| peer.linked().map(|entity| (entity, peer)))
    }

    /// Closes every connection with `AppCode::STOPPED`, and takes no new one; the thread writes the
    /// closes, then ends.
    pub(crate) fn close_all(&mut self, now: Instant) {
        self.closing = true;
        self.admission = None;
        for peer in self.peers.iter_mut().flatten() {
            peer.close(now, AppCode::STOPPED);
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    #[cfg(test)]
    use bevy_ecs::entity::Entity;

    use crate::endpoint_state::EndpointState;

    impl EndpointState {
        /// Drops the datagrams each connection received, linked or not: how many.
        pub(crate) fn take_datagrams(&mut self) -> usize {
            let mut taken = 0;
            for peer in self.peers.iter_mut().flatten() {
                while peer.connection.datagrams().recv().is_some() {
                    taken += 1;
                }
            }
            taken
        }
    }

    #[cfg(test)]
    impl EndpointState {
        pub(crate) const fn retries(&self) -> usize {
            self.retries
        }

        pub(crate) fn has_news(&self) -> bool {
            !self.news.is_empty()
        }

        /// Drops the connection the entity `entity` carries with no close, as a peer that stops
        /// sends none; the endpoint keeps nothing of it.
        pub(crate) fn forget(&mut self, entity: Entity) {
            for slot in &mut self.peers {
                if slot
                    .as_ref()
                    .is_some_and(|peer| peer.entity() == Some(entity))
                {
                    *slot = None;
                }
            }
        }
    }
}
