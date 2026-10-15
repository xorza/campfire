use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::query::With;
use bevy_ecs::system::{Commands, Query};
use bevy_ecs::world::DeferredWorld;
use campfire_common::TlsKey;
use campfire_log::ErrorReport;
use campfire_store::Worker;
use lightyear::core::id::{PeerId, RemoteId};
use lightyear::core::time::Instant as LinkInstant;
use lightyear::link::server::LinkOf;
use lightyear::link::{Link, LinkMtu, Linked, Linking, UnlinkReason, Unlinked};
use lightyear::prelude::{LocalAddr, PeerAddr};
use quinn_proto::crypto::HandshakeTokenKey;
use quinn_proto::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn_proto::{
    ClientConfig, Endpoint, EndpointConfig, RandomConnectionIdGenerator, SendDatagramError,
    ServerConfig, ValidationTokenConfig,
};
use ring::hkdf;
use ring::rand::{SecureRandom, SystemRandom};
use tracing::{trace, warn};
use zeroize::Zeroizing;

use crate::address_limit::AddressLimit;
use crate::app_code::AppCode;
use crate::endpoint_state::{EndpointState, News};
use crate::error::BindError;
use crate::io_thread::IoThread;
use crate::link_end::LinkEnd;
use crate::link_times::LinkTimes;
use crate::peer::Peer;
use crate::pinned_key_verifier::PinnedKeyVerifier;
use crate::quic_link::QuicLink;
use crate::reset_key::ResetKey;
use crate::server_key::ServerKey;
use crate::shared_endpoint::SharedEndpoint;
use crate::udp_port::UdpPort;
use crate::used_tokens::UsedTokens;
use crate::{CID_LEN, LINK_MIN_MTU};

/// The `NEW_TOKEN` tokens a connection gives its client: one for its next connection, and one more
/// should that one fail before its own handshake gives new ones. `quinn-proto` queues them as the
/// Retry validates the client's address and again as the handshake does, so a client may get
/// them twice; it keeps the last two.
const TOKENS_SENT: u32 = 2;
/// How long a `NEW_TOKEN` token lasts: two minutes, a session's default restore window, the time a
/// client tries to rejoin. A server's start makes every earlier token useless by its new key.
const TOKEN_LIFETIME: Duration = Duration::from_secs(120);

/// A QUIC endpoint on a server's entity or a client's: its socket, its connections and the
/// thread that drives them (design 15, D3, D9). A `LinkStart` opens it: a server takes
/// connections within its `AddressLimit` (D13), and spawns a `LinkOf` for each once its
/// handshake completes; a client connects to its server, and its entity carries the connection. Taken from a started entity, by any path, it ends that entity's link;
/// dropped, it closes every connection with `AppCode::STOPPED`, waits until its thread wrote the
/// closes, and joins it.
#[derive(Component, Debug)]
#[component(on_discard = QuicEndpoint::on_discard)]
pub struct QuicEndpoint {
    shared: Arc<SharedEndpoint>,
    side: Side,
    local: SocketAddr,
    _worker: Worker,
}

/// Whether the endpoint takes connections, presenting its TLS key, or opens one to its server,
/// whose key it pinned.
#[derive(Debug)]
enum Side {
    Server {
        key: TlsKey,
        limit: AddressLimit,
    },
    Client {
        server: SocketAddr,
        pin: TlsKey,
        config: ClientConfig,
    },
}

impl QuicEndpoint {
    /// A link of `LINK_MIN_MTU`, as every link of a session starts, over QUIC or in process: its
    /// transport fixes its fragments by it then.
    pub fn link() -> Link {
        Link::default().with_mtu(LinkMtu::new(LINK_MIN_MTU))
    }

    /// A client's endpoint, which connects to `server` and takes only the key `pin`.
    pub fn client(
        server: SocketAddr,
        pin: &TlsKey,
        times: LinkTimes,
    ) -> Result<QuicEndpoint, BindError> {
        let config =
            QuicEndpoint::client_config(PinnedKeyVerifier::new(pin).client_config(), times);
        QuicEndpoint::client_of(server, *pin, config, CID_LEN)
    }

    fn client_of(
        server: SocketAddr,
        pin: TlsKey,
        config: ClientConfig,
        cid_len: usize,
    ) -> Result<QuicEndpoint, BindError> {
        let unspecified = if server.is_ipv4() {
            Ipv4Addr::UNSPECIFIED.into()
        } else {
            Ipv6Addr::UNSPECIFIED.into()
        };
        let side = Side::Client {
            server,
            pin,
            config,
        };
        QuicEndpoint::open(
            SocketAddr::new(unspecified, 0),
            None,
            QuicEndpoint::endpoint_config(ResetKey::random(), cid_len),
            side,
        )
    }

    /// The server's QUIC: a Retry for each new address, and `NEW_TOKEN` tokens after each
    /// handshake, each of which lets the client's next connection from its IP skip the Retry once
    /// within `TOKEN_LIFETIME` (D8).
    fn server_config(crypto: Arc<QuicServerConfig>, times: LinkTimes) -> ServerConfig {
        let mut config = ServerConfig::new(crypto, QuicEndpoint::token_key());
        config.transport_config(Arc::new(times.transport()));
        let mut tokens = ValidationTokenConfig::default();
        tokens
            .sent(TOKENS_SENT)
            .lifetime(TOKEN_LIFETIME)
            .log(Arc::new(UsedTokens::default()));
        config.validation_token_config(tokens);
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
        Ok(QuicEndpoint::run(port, local, server, config, side))
    }

    /// The endpoint of `port`, bound to `local`, with its thread running.
    fn run(
        port: UdpPort,
        local: SocketAddr,
        server: Option<Arc<ServerConfig>>,
        config: EndpointConfig,
        side: Side,
    ) -> QuicEndpoint {
        let max_payload =
            usize::try_from(config.get_max_udp_payload_size()).expect("a UDP payload fits usize");
        let endpoint = Endpoint::new(Arc::new(config), server, !port.may_fragment(), None);
        let state = EndpointState::new(endpoint, port.max_gso_segments());
        let shared = Arc::new(SharedEndpoint::new(state, port));
        let thread = IoThread::new(Arc::clone(&shared), max_payload);
        let worker = Worker::start("campfire-quic", move || thread.run());
        QuicEndpoint {
            shared,
            side,
            local,
            _worker: worker,
        }
    }

    /// The address the socket is bound to, its port the OS's when the address named 0.
    pub const fn local_address(&self) -> SocketAddr {
        self.local
    }

    /// The TLS key of the endpoint's connections: the one a server presents, which its listing
    /// names, or the one a client pinned.
    pub const fn tls_key(&self) -> TlsKey {
        match &self.side {
            Side::Server { key, .. } => *key,
            Side::Client { pin, .. } => *pin,
        }
    }

    /// The key of the Retry and `NEW_TOKEN` tokens, new at each start: a token outlives no server.
    fn token_key() -> Arc<dyn HandshakeTokenKey> {
        let mut secret = Zeroizing::new([0; 32]);
        SystemRandom::new()
            .fill(secret.as_mut_slice())
            .expect("the OS gives random bytes");
        Arc::new(hkdf::Salt::new(hkdf::HKDF_SHA256, &[]).extract(secret.as_slice()))
    }

    /// Opens the endpoint on its entity `entity`, whose `link` a client's connection fills: a
    /// server takes connections within its limit for each address, and is linked at once; a
    /// client connects, and is linked once its handshake completes.
    pub(crate) fn start(
        &self,
        entity: Entity,
        link: Option<&Link>,
        commands: &mut Commands<'_, '_>,
    ) {
        let (server, config) = match &self.side {
            Side::Client { server, config, .. } => (server, config),
            Side::Server { limit, .. } => {
                self.shared.lock().start_accepting(*limit, Instant::now());
                commands
                    .entity(entity)
                    .insert((LocalAddr(self.local), Linked));
                return;
            }
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
            Ok(key) => {
                commands.entity(entity).insert((
                    QuicLink::new(Arc::downgrade(&self.shared), key),
                    LocalAddr(self.local),
                    PeerAddr(*server),
                    Linking,
                ));
                self.shared.wake();
            }
            Err(error) => {
                warn!(error = %ErrorReport::of(&error), %server, "the client does not connect");
                QuicEndpoint::unlink(entity, LinkEnd::NotOpened, commands);
            }
        }
    }

    /// Ends a server's endpoint's taking of connections, as its `Unlink` ends the server, whose
    /// links Lightyear despawns, which closes them; a connection no link carries yet closes with
    /// `AppCode::ENDED`. A client's endpoint takes none.
    pub(crate) fn stop(&self) {
        if let Side::Server { .. } = self.side {
            self.shared.lock().stop_accepting(Instant::now());
            self.shared.wake();
        }
    }

    /// Takes the thread's news into the world, the endpoint on `endpoint`, and moves each linked
    /// connection's datagrams to its link's receive queue, its MTU and its address to its link.
    pub(crate) fn receive(
        &self,
        endpoint: Entity,
        links: &mut Query<'_, '_, (&mut Link, &mut PeerAddr)>,
        commands: &mut Commands<'_, '_>,
    ) {
        let mut state = self.shared.lock();
        while let Some(news) = state.next_news() {
            match news {
                News::Connected { key } => {
                    if let Some(peer) = state.peer(key) {
                        self.link_peer(endpoint, peer, commands);
                    }
                }
                News::Lost { entity, end, .. } => QuicEndpoint::unlink(entity, end, commands),
            }
        }
        let arrived = LinkInstant::now();
        for (entity, peer) in state.linked() {
            let Ok((mut link, mut address)) = links.get_mut(entity) else {
                continue;
            };
            let mtu = peer
                .link_mtu()
                .expect("the thread closes a connection whose datagrams cannot carry a link");
            link.set_mtu(mtu)
                .expect("a QUIC link's minimum is LINK_MIN_MTU");
            while let Some(datagram) = peer.connection.datagrams().recv() {
                link.recv.push(datagram, arrived);
            }
            let remote = peer.remote();
            if address.0 != remote {
                address.0 = remote;
            }
        }
    }

    /// Links a connection whose handshake completed: the client's entity, or a new `LinkOf` of
    /// the server on `endpoint`, spawned whole, its peer id its own entity (D9). A connection
    /// closed since is not linked: a client's loss is in the news after it, and a server's has no
    /// entity to tell.
    fn link_peer(&self, endpoint: Entity, peer: &mut Peer, commands: &mut Commands<'_, '_>) {
        if peer.connection.is_closed() {
            return;
        }
        let remote = peer.remote();
        let binding = peer.binding();
        let entity = match self.side {
            Side::Client { .. } => {
                let client = peer
                    .entity()
                    .expect("a client's open connection keeps its entity until it closes");
                commands
                    .entity(client)
                    .insert((PeerAddr(remote), binding, Linked));
                client
            }
            Side::Server { .. } => {
                let link = commands
                    .spawn((
                        LinkOf { server: endpoint },
                        QuicEndpoint::link(),
                        PeerAddr(remote),
                        binding,
                        QuicLink::new(Arc::downgrade(&self.shared), peer.key()),
                    ))
                    .id();
                commands
                    .entity(link)
                    .insert((RemoteId(PeerId::Entity(link.to_bits())), Linked));
                link
            }
        };
        peer.link(entity);
    }

    /// Unlinks the link `entity`, which ended as `end` says, with it on the entity for the
    /// observers of `Unlinked` to read; Lightyear's reason holds its text, for its logs.
    fn unlink(entity: Entity, end: LinkEnd, commands: &mut Commands<'_, '_>) {
        if let Ok(mut link) = commands.get_entity(entity) {
            link.try_remove::<QuicLink>();
            link.try_insert((
                end,
                Unlinked {
                    reason: UnlinkReason::TransportError(format!("{end:?}")),
                },
            ));
        }
    }

    /// Ends the link of a started endpoint's entity as the endpoint leaves it, by a removal, a new
    /// endpoint in its place or a despawn; its drop then closes each connection. A client's entity
    /// ends `Closed` with `AppCode::STOPPED`, the code it sent; a server's entity unlinks, and
    /// Lightyear unlinks and despawns each of its links.
    fn on_discard(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        if world.get::<Linked>(entity).is_none() && world.get::<Linking>(entity).is_none() {
            return;
        }
        let end = LinkEnd::Closed(AppCode::STOPPED);
        let endpoint = world
            .get::<QuicEndpoint>(entity)
            .expect("a hook of QuicEndpoint runs on an entity that holds one");
        let client = matches!(endpoint.side, Side::Client { .. });
        let mut commands = world.commands();
        if client {
            QuicEndpoint::unlink(entity, end, &mut commands);
        } else {
            commands.entity(entity).try_insert(Unlinked {
                reason: UnlinkReason::TransportError(format!("{end:?}")),
            });
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

/// A server's socket, bound, and what its endpoint takes: it serves once its limit for each
/// address is known, so no server's endpoint exists without one.
#[derive(Debug)]
pub struct ServerSocket {
    port: UdpPort,
    local: SocketAddr,
    server: Arc<ServerConfig>,
    config: EndpointConfig,
    key: TlsKey,
}

impl ServerSocket {
    /// A server's socket on `address`, whose TLS key and reset key both derive from `secret`,
    /// the secret of the server's own key (D6, D8). It binds now, before its app runs, so a port
    /// in use is this error, not a server with no link (D2); it serves once its limit for each
    /// address is known.
    pub fn bind(
        address: SocketAddr,
        secret: &Zeroizing<[u8; 32]>,
        times: LinkTimes,
    ) -> Result<ServerSocket, BindError> {
        let key = ServerKey::derive(secret);
        let config = QuicEndpoint::server_config(key.server_config(), times);
        let port = UdpPort::bind(address).map_err(|error| BindError { address, error })?;
        let local = port
            .local_address()
            .map_err(|error| BindError { address, error })?;
        Ok(ServerSocket {
            port,
            local,
            server: Arc::new(config),
            config: QuicEndpoint::endpoint_config(ResetKey::derive(secret), CID_LEN),
            key: key.public(),
        })
    }

    /// The address the socket is bound to, its port the OS's when the address named 0.
    pub const fn local_address(&self) -> SocketAddr {
        self.local
    }

    /// The TLS key the server presents, which its listing names.
    pub const fn tls_key(&self) -> TlsKey {
        self.key
    }

    /// The server's endpoint, which takes connections within `limit` for each address once it
    /// starts.
    pub fn serve(self, limit: AddressLimit) -> QuicEndpoint {
        let ServerSocket {
            port,
            local,
            server,
            config,
            key,
        } = self;
        QuicEndpoint::run(
            port,
            local,
            Some(server),
            config,
            Side::Server { key, limit },
        )
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

#[cfg(feature = "internals")]
pub(crate) mod thread_fixture;

#[cfg(test)]
mod tests;
