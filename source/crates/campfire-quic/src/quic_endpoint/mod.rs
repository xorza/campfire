use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::system::{Commands, Query};
use bytes::Bytes;
use campfire_log::ErrorReport;
use campfire_store::Worker;
use lightyear::core::id::{PeerId, RemoteId};
use lightyear::core::time::Instant as LinkInstant;
use lightyear::link::server::LinkOf;
use lightyear::link::{
    Link, LinkMtu, Linked, Linking, UnlinkReason, Unlinked, recv_payload_from_bytes,
};
use lightyear::prelude::{LocalAddr, PeerAddr};
use quinn_proto::crypto::HandshakeTokenKey;
use quinn_proto::crypto::rustls::QuicClientConfig;
use quinn_proto::{
    ClientConfig, Endpoint, EndpointConfig, RandomConnectionIdGenerator, SendDatagramError,
    ServerConfig,
};
use ring::hkdf;
use ring::rand::{SecureRandom, SystemRandom};
use tracing::{trace, warn};
use zeroize::Zeroizing;

use crate::LINK_MIN_MTU;
use crate::endpoint_state::{EndpointState, News, Peer, UNFIT};
use crate::error::BindError;
use crate::io_thread::IoThread;
use crate::link_times::LinkTimes;
use crate::pinned_key_verifier::PinnedKeyVerifier;
use crate::quic_link::QuicLink;
use crate::reset_key::ResetKey;
use crate::server_key::ServerKey;
use crate::server_public_key::ServerPublicKey;
use crate::shared_endpoint::SharedEndpoint;
use crate::udp_port::UdpPort;

/// The length of every connection id Campfire's endpoints give, which `LINK_MIN_MTU` counts
/// (design 15, D5).
const CID_LEN: usize = 8;

/// A QUIC endpoint on a server's entity or a client's: its socket, its connections and the
/// thread that drives them (design 15, D3, D9). A `LinkStart` opens it: a server takes
/// connections and spawns a `LinkOf` for each once its handshake completes; a client connects
/// to its server, and its entity carries the connection. Dropped, it closes every connection,
/// waits until its thread wrote the closes, and joins it.
#[derive(Component, Debug)]
pub struct QuicEndpoint {
    shared: Arc<SharedEndpoint>,
    side: Side,
    local: SocketAddr,
    _worker: Worker,
}

/// Whether the endpoint takes connections, or opens one to its server.
#[derive(Debug)]
enum Side {
    Server,
    Client {
        server: SocketAddr,
        config: ClientConfig,
    },
}

impl QuicEndpoint {
    /// A server's endpoint on `address`, presenting `key`, its resets keyed by `reset`. It binds
    /// now, before its app runs, so a port in use is this error, not a server with no link (D2).
    pub fn server(
        address: SocketAddr,
        key: &ServerKey,
        reset: ResetKey,
        times: LinkTimes,
    ) -> Result<QuicEndpoint, BindError> {
        let config = QuicEndpoint::server_config(key, times);
        QuicEndpoint::open(
            address,
            Some(Arc::new(config)),
            QuicEndpoint::endpoint_config(reset, CID_LEN),
            Side::Server,
        )
    }

    /// A client's endpoint, which connects to `server` and takes only the key `pin`.
    pub fn client(
        server: SocketAddr,
        pin: ServerPublicKey,
        times: LinkTimes,
    ) -> Result<QuicEndpoint, BindError> {
        let config =
            QuicEndpoint::client_config(PinnedKeyVerifier::new(pin).client_config(), times);
        QuicEndpoint::client_of(server, config, CID_LEN)
    }

    fn client_of(
        server: SocketAddr,
        config: ClientConfig,
        cid_len: usize,
    ) -> Result<QuicEndpoint, BindError> {
        let unspecified = if server.is_ipv4() {
            Ipv4Addr::UNSPECIFIED.into()
        } else {
            Ipv6Addr::UNSPECIFIED.into()
        };
        let side = Side::Client { server, config };
        QuicEndpoint::open(
            SocketAddr::new(unspecified, 0),
            None,
            QuicEndpoint::endpoint_config(ResetKey::random(), cid_len),
            side,
        )
    }

    fn server_config(key: &ServerKey, times: LinkTimes) -> ServerConfig {
        let mut config = ServerConfig::new(key.server_config(), QuicEndpoint::token_key());
        config.transport_config(Arc::new(times.transport()));
        config
    }

    fn client_config(crypto: Arc<QuicClientConfig>, times: LinkTimes) -> ClientConfig {
        let mut config = ClientConfig::new(crypto);
        config.transport_config(Arc::new(times.transport()));
        config
    }

    fn endpoint_config(reset: ResetKey, cid_len: usize) -> EndpointConfig {
        let mut config = EndpointConfig::new(reset.into_hmac());
        config.cid_generator(move || Box::new(RandomConnectionIdGenerator::new(cid_len)));
        config
    }

    fn open(
        address: SocketAddr,
        server: Option<Arc<ServerConfig>>,
        config: EndpointConfig,
        side: Side,
    ) -> Result<QuicEndpoint, BindError> {
        let bind_error = |error| BindError { address, error };
        let port = UdpPort::bind(address).map_err(bind_error)?;
        let local = port.local_address().map_err(bind_error)?;
        let max_payload =
            usize::try_from(config.get_max_udp_payload_size()).expect("a UDP payload fits usize");
        let endpoint = Endpoint::new(Arc::new(config), server, !port.may_fragment(), None);
        let state = EndpointState::new(endpoint, port.max_gso_segments());
        let shared = Arc::new(SharedEndpoint::new(state, port));
        let thread = IoThread::new(Arc::clone(&shared), max_payload);
        let worker = Worker::start("campfire-quic", move || thread.run());
        Ok(QuicEndpoint {
            shared,
            side,
            local,
            _worker: worker,
        })
    }

    /// The address the socket is bound to, its port the OS's when the address named 0.
    pub const fn local_address(&self) -> SocketAddr {
        self.local
    }

    /// The key of the Retry tokens, new at each start: a token outlives no server.
    fn token_key() -> Arc<dyn HandshakeTokenKey> {
        let mut secret = Zeroizing::new([0; 32]);
        SystemRandom::new()
            .fill(secret.as_mut_slice())
            .expect("the OS gives random bytes");
        Arc::new(hkdf::Salt::new(hkdf::HKDF_SHA256, &[]).extract(secret.as_slice()))
    }

    /// Opens the endpoint on its entity `entity`, whose `link` a client's connection fills: a
    /// server is linked at once; a client connects, and is linked once its handshake completes.
    pub(crate) fn start(
        &self,
        entity: Entity,
        link: Option<&Link>,
        commands: &mut Commands<'_, '_>,
    ) {
        let Side::Client { server, config } = &self.side else {
            self.shared.lock().accept(true);
            commands
                .entity(entity)
                .insert((LocalAddr(self.local), Linked));
            return;
        };
        let link = link.expect("a client's endpoint is on the entity of its link");
        assert_eq!(
            link.min_mtu(),
            LINK_MIN_MTU,
            "a client's link takes LINK_MIN_MTU from its spawn, as its transport fixes its fragments then"
        );
        let connected = self
            .shared
            .lock()
            .connect(Instant::now(), config.clone(), *server, entity);
        match connected {
            Ok((handle, id)) => {
                commands.entity(entity).insert((
                    QuicLink::new(Arc::downgrade(&self.shared), handle, id),
                    LocalAddr(self.local),
                    PeerAddr(*server),
                    Linking,
                ));
                self.shared.wake();
            }
            Err(error) => {
                warn!(error = %ErrorReport::of(&error), %server, "the client does not connect");
                commands.entity(entity).insert(Unlinked {
                    reason: UnlinkReason::TransportError(ErrorReport::of(&error).to_string()),
                });
            }
        }
    }

    /// Ends a server's endpoint's taking of connections, as its `Unlink` ends the server, whose
    /// links Lightyear despawns, which closes them.
    pub(crate) fn stop(&self) {
        self.shared.lock().accept(false);
    }

    /// Takes the thread's news into the world, the endpoint on `entity`, and moves each linked
    /// connection's datagrams to its link's receive queue, its MTU and its address to its link.
    pub(crate) fn receive(
        &self,
        entity: Entity,
        links: &mut Query<'_, '_, (&mut Link, &mut PeerAddr)>,
        commands: &mut Commands<'_, '_>,
        news: &mut Vec<News>,
    ) {
        let now = Instant::now();
        let mut closed = false;
        let mut state = self.shared.lock();
        state.take_news(news);
        for item in news.drain(..) {
            match item {
                News::Connected {
                    handle,
                    id,
                    max_size,
                } => {
                    let Some(peer) = state.peer(handle, id) else {
                        continue;
                    };
                    let fit = max_size.is_some_and(|max| max >= LINK_MIN_MTU);
                    closed |= self.link(entity, peer, fit, now, commands);
                }
                News::Lost { entity, reason } => {
                    QuicEndpoint::unlink(
                        entity,
                        UnlinkReason::TransportError(ErrorReport::of(&reason).to_string()),
                        commands,
                    );
                }
            }
        }
        let arrived = LinkInstant::now();
        for (entity, peer) in state.linked() {
            let Ok((mut link, mut address)) = links.get_mut(entity) else {
                continue;
            };
            let Some(max) = QuicEndpoint::fit(peer) else {
                peer.close(now, entity, UNFIT);
                QuicEndpoint::unlink(entity, QuicEndpoint::unfit(), commands);
                closed = true;
                continue;
            };
            link.set_mtu(max)
                .expect("a fit connection's datagram is at least the link's minimum");
            while let Some(datagram) = peer.connection.datagrams().recv() {
                link.recv.push(recv_payload_from_bytes(datagram), arrived);
            }
            let remote = peer.remote();
            if address.0 != remote {
                address.0 = remote;
            }
        }
        drop(state);
        if closed {
            self.shared.wake();
        }
    }

    /// Links a connection whose handshake completed: the client's entity, or a new `LinkOf` of
    /// the server on `endpoint`, spawned whole, its peer id its own entity (D9). A connection lost
    /// since is not linked: a client's loss is in the news after it, and a server's had no entity
    /// to tell. One that was not `fit`, whose datagrams on QUIC's minimum path cannot carry
    /// `LINK_MIN_MTU`, is closed instead; whether it was.
    fn link(
        &self,
        endpoint: Entity,
        peer: &mut Peer,
        fit: bool,
        now: Instant,
        commands: &mut Commands<'_, '_>,
    ) -> bool {
        if peer.connection.is_closed() {
            return false;
        }
        if !fit {
            if let Some(client) = peer.entity() {
                peer.close(now, client, UNFIT);
                QuicEndpoint::unlink(client, QuicEndpoint::unfit(), commands);
            } else {
                peer.connection.close(now, UNFIT, Bytes::new());
            }
            return true;
        }
        let remote = peer.remote();
        let entity = match self.side {
            Side::Client { .. } => {
                let client = peer
                    .entity()
                    .expect("a client's open connection keeps its entity until it closes");
                commands.entity(client).insert((PeerAddr(remote), Linked));
                client
            }
            Side::Server => {
                let link = commands
                    .spawn((
                        LinkOf { server: endpoint },
                        Link::default().with_mtu(LinkMtu::new(LINK_MIN_MTU)),
                        PeerAddr(remote),
                        QuicLink::new(Arc::downgrade(&self.shared), peer.handle(), peer.id()),
                    ))
                    .id();
                commands
                    .entity(link)
                    .insert((RemoteId(PeerId::Entity(link.to_bits())), Linked));
                link
            }
        };
        peer.link(entity);
        false
    }

    /// The connection's largest datagram now, while it carries `LINK_MIN_MTU`: a Campfire peer's
    /// always does, and only a peer that gives longer connection ids or a smaller
    /// `max_datagram_frame_size` makes it less (D5).
    fn fit(peer: &mut Peer) -> Option<usize> {
        peer.connection
            .datagrams()
            .max_size()
            .filter(|&max| max >= LINK_MIN_MTU)
    }

    fn unfit() -> UnlinkReason {
        UnlinkReason::TransportError(
            "the connection's datagrams cannot carry the link's minimum MTU".to_owned(),
        )
    }

    fn unlink(entity: Entity, reason: UnlinkReason, commands: &mut Commands<'_, '_>) {
        if let Ok(mut link) = commands.get_entity(entity) {
            link.remove::<QuicLink>();
            link.insert(Unlinked { reason });
        }
    }

    /// Moves each linked connection's packets from its link's send queue to the connection, then
    /// wakes the thread, which writes them: one wake for each frame (D3). A packet larger than the
    /// connection's datagram, as black hole detection lowered it since the link's packets were
    /// made, is dropped, and Lightyear sends a reliable one again.
    pub(crate) fn send(&self, links: &mut Query<'_, '_, &mut Link, With<Linked>>) {
        let mut sent = false;
        {
            let mut state = self.shared.lock();
            for (entity, peer) in state.linked() {
                let Ok(mut link) = links.get_mut(entity) else {
                    continue;
                };
                for packet in link.send.drain() {
                    sent = true;
                    match peer.connection.datagrams().send(packet, true) {
                        Ok(()) => {}
                        Err(SendDatagramError::TooLarge) => {
                            trace!("a packet is larger than its connection's datagram");
                        }
                        Err(error) => {
                            unreachable!(
                                "a linked connection carries datagrams: {}",
                                ErrorReport::of(&error)
                            )
                        }
                    }
                }
            }
        }
        if sent {
            self.shared.wake();
        }
    }
}

impl Drop for QuicEndpoint {
    fn drop(&mut self) {
        self.shared.lock().close_all(Instant::now());
        self.shared.wake();
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use std::sync::MutexGuard;

    use super::*;

    impl QuicEndpoint {
        pub(crate) fn state(&self) -> MutexGuard<'_, EndpointState> {
            self.shared.lock()
        }
    }
}

#[cfg(test)]
mod tests;
