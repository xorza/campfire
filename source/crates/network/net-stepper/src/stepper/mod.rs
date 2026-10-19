use bevy_app::{App, FixedUpdate, PluginsState, TaskPoolPlugin};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::With;
use bevy_ecs::system::{Commands, Query};
use bevy_ecs::world::{EntityRef, EntityWorldMut, World};
use bevy_time::{Real, Time, TimePlugin, TimeUpdateStrategy};
use campfire_net_connection::{
    Client, Connect, Connected, Connecting, Disconnect, Disconnected, RawClient, RawServer, Start,
};
use campfire_net_core::NetTick;
use campfire_net_core::TickRate;
use campfire_net_emulator::{
    DuplexModel, EmulatedLinkId, EmulatedNetwork, EmulatedPair, WireClock,
};
use campfire_net_endpoint::Endpoint;
use campfire_net_link::{Link, LinkOf, Linked, PeerAddr, Unlinked};
use campfire_net_packets::{EmulatedWires, EndpointConfig, EndpointParts, SessionConfig};
use campfire_net_timeline::LocalTimeline;
use campfire_net_udp::{
    AdmissionConfig, EmulatedServer, ServerIdentity, ServerSecret, ServerTrust, UdpConfig,
};
use core::num::NonZeroU32;
use core::time::Duration;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::error_span;
use zeroize::Zeroizing;

use crate::app_id::AppId;
use crate::app_schedule::AppSchedule;
use crate::client_kind::ClientKind;
use crate::remote_client::RemoteClient;
use crate::stack::Stack;
use crate::stepper_config::{Init, StepperConfig};

/// The ticks `init` and `disconnect` step before they give up on a client.
const WAIT_TICKS: usize = 50;

/// The ALPN of every UDP link of a stepper.
const UDP_ALPN: &[u8] = b"campfire-stepper";

/// The secret of the server's raw key, which each UDP client pins.
const UDP_SECRET: [u8; 32] = [7; 32];

/// The order in which the apps of one step run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    ClientsFirst,
    ServerFirst,
}

/// One server app and one app for each remote client, linked over a [`EmulatedNetwork`], with the
/// stack `S` on each side; a host client lives in the server's app.
///
/// Every remote client is of one kind: a plain session or a sealed one of the UDP protocol, which
/// the server's one endpoint takes. The server's endpoint spawns its link to each client at the
/// client's first packet, and the stack's components go on it before it links.
///
/// The stepper owns one virtual time. `step` and `advance_time` move every app's clock together;
/// `run_for` runs each app on its own frame interval, clock drift and stalls, frame by frame in
/// the order their frames fall. Each app's stack clock is on its default `ClockSource::Auto`,
/// which follows the time the stepper hands to its `TimeUpdateStrategy`, so every endpoint runs
/// its pass in the frame.
#[derive(Debug)]
pub struct Stepper<S> {
    stack: S,
    server_app: App,
    server_entity: Entity,
    clients: Vec<RemoteClient>,
    /// The kind of every remote client.
    remote: ClientKind,
    host_client_entity: Option<Entity>,
    frame_interval: Duration,
    tick_rate: TickRate,
    /// The OS moment the stepper's time counts from, which every app's driven time is offset by.
    origin: Instant,
    /// The stepper's virtual time.
    now: Duration,
    server_schedule: AppSchedule,
    /// The network every remote client links over, on the stepper's time.
    network: EmulatedNetwork,
    /// How each new remote client's link carries packets.
    link: DuplexModel,
    /// Client `i`'s link draws follow `seed + i`.
    seed: u64,
    /// The sessions of every endpoint.
    session: SessionConfig,
    server_started: bool,
}

impl<S: Stack> Stepper<S> {
    pub fn new(stack: S, config: StepperConfig) -> Self {
        assert!(
            config.frame_interval > Duration::ZERO,
            "a frame interval is positive"
        );
        let host_clients = config
            .clients
            .iter()
            .filter(|&&kind| kind == ClientKind::Host)
            .count();
        assert!(host_clients <= 1, "a server has at most one host client");
        let remote = if config.clients.contains(&ClientKind::Udp) {
            assert!(
                !config.clients.contains(&ClientKind::Raw),
                "the server's one endpoint takes plain or sealed sessions, not both"
            );
            ClientKind::Udp
        } else {
            ClientKind::Raw
        };
        let mut server_app = App::new();
        server_app.add_plugins((TaskPoolPlugin::default(), TimePlugin));
        stack.add_server(&mut server_app, config.tick_rate);
        // The links the server's endpoint spawns take the stack's components before they link: a
        // queued command runs before the endpoint's `Linked`, which comes in a later command.
        server_app.add_observer(
            move |link: On<'_, '_, Add<LinkOf>>,
                  endpoint_links: Query<'_, '_, (), With<PeerAddr>>,
                  mut commands: Commands<'_, '_>| {
                let entity = link.entity;
                if endpoint_links.contains(entity) {
                    commands.queue(move |world: &mut World| {
                        stack.server_link(&mut world.entity_mut(entity));
                    });
                }
            },
        );
        if host_clients == 1 {
            stack.add_client(&mut server_app, config.tick_rate);
        }
        stack.add_protocol(&mut server_app);
        let endpoint = Endpoint::from(match remote {
            ClientKind::Udp => EmulatedServer::new(
                Self::udp_identity(),
                Self::udp_config(config.session),
                config.seed,
            )
            .serve(AdmissionConfig::new(
                NonZeroU32::MAX,
                Duration::from_secs(1),
            )),
            ClientKind::Raw | ClientKind::Host => EndpointParts::plain_server(EndpointConfig {
                session: config.session,
                conditioner: None,
            }),
        });
        let server_entity = server_app.world_mut().spawn((RawServer, endpoint)).id();
        let host_client_entity = (host_clients == 1).then(|| {
            server_app
                .world_mut()
                .spawn((
                    Client,
                    LinkOf {
                        server: server_entity,
                    },
                    Link::default(),
                    Linked,
                ))
                .id()
        });

        let mut stepper = Self {
            stack,
            server_app,
            server_entity,
            clients: vec![],
            remote,
            host_client_entity,
            frame_interval: config.frame_interval,
            tick_rate: config.tick_rate,
            origin: Instant::now(),
            now: Duration::ZERO,
            server_schedule: AppSchedule::new(config.frame_interval, Duration::ZERO),
            network: EmulatedNetwork::new(WireClock::Driven),
            link: config.link,
            seed: config.seed,
            session: config.session,
            server_started: false,
        };
        for kind in &config.clients {
            if *kind != ClientKind::Host {
                stepper.add_remote_client();
            }
        }
        if config.init == Init::Ready {
            stepper.init();
        }
        stepper
    }

    /// Adds a remote client of the stepper's kind with its own app and link, and returns its
    /// index.
    pub fn add_remote_client(&mut self) -> usize {
        let mut app = self.new_client_app();
        let client_id = self.clients.len();
        let pair = self.emulated_pair(client_id);
        let wire = pair.client.link();
        let address = pair.client.client_addr();
        let endpoint = self.client_endpoint(client_id, pair);
        let mut client = app
            .world_mut()
            .spawn((Client, RawClient, Link::default(), endpoint));
        self.stack.client_link(&mut client);
        let entity = client.id();
        self.clients.push(RemoteClient {
            app,
            entity,
            address,
            wire,
            schedule: AppSchedule::new(self.frame_interval, self.now),
        });
        client_id
    }

    /// The client end of `pair` for remote client `client_id`, whose server end goes to the
    /// server's endpoint.
    fn client_endpoint(&mut self, client_id: usize, pair: EmulatedPair) -> Endpoint<EmulatedWires> {
        self.server_mut()
            .get::<Endpoint<EmulatedWires>>()
            .expect("the server holds its endpoint")
            .io()
            .add(pair.server);
        match self.remote {
            ClientKind::Udp => {
                let trust = ServerTrust::pinned(
                    Self::udp_identity()
                        .server_key()
                        .expect("the stepper's server presents a raw key"),
                );
                let seed = self.seed.wrapping_add(u64::try_from(client_id).unwrap());
                let config = Self::udp_config(self.session);
                Endpoint::from(campfire_net_udp::emulated_client(
                    pair.client,
                    &trust,
                    &config,
                    seed,
                ))
            }
            ClientKind::Raw | ClientKind::Host => Endpoint::from(EndpointParts::plain_client(
                pair.client,
                EndpointConfig {
                    session: self.session,
                    conditioner: None,
                },
            )),
        }
    }

    /// The identity of the server's UDP endpoint: a raw key, the same at each run.
    fn udp_identity() -> ServerIdentity {
        ServerIdentity::raw_key(ServerSecret::new(Zeroizing::new(UDP_SECRET)))
    }

    fn udp_config(session: SessionConfig) -> UdpConfig {
        UdpConfig {
            session,
            ..UdpConfig::new(UDP_ALPN)
        }
    }

    /// A remote client's app with the stack's plugins and protocol, finished.
    fn new_client_app(&self) -> App {
        let mut client_app = App::new();
        client_app.add_plugins((TaskPoolPlugin::default(), TimePlugin));
        self.stack.add_client(&mut client_app, self.tick_rate);
        self.stack.add_protocol(&mut client_app);
        client_app.finish();
        client_app.cleanup();
        client_app
    }

    /// A new simulated link for remote client `client_id`, its models seeded by the index.
    fn emulated_pair(&self, client_id: usize) -> EmulatedPair {
        self.network.pair(
            self.link
                .clone()
                .with_seed(self.seed.wrapping_add(u64::try_from(client_id).unwrap())),
        )
    }

    /// Connects remote client `client` again over a new simulated link and a new endpoint,
    /// without stepping. The server's link to it is a new entity, as a reconnect is: the
    /// connection layer despawned the old one when it disconnected.
    pub fn reconnect(&mut self, client: usize) {
        let client_entity = self.clients[client].entity;
        let pair = self.emulated_pair(client);
        self.clients[client].wire = pair.client.link();
        self.clients[client].address = pair.client.client_addr();
        let endpoint = self.client_endpoint(client, pair);
        self.clients[client]
            .app
            .world_mut()
            .entity_mut(client_entity)
            .insert(endpoint);
        self.clients[client]
            .app
            .world_mut()
            .trigger(Connect::from(client_entity));
    }

    /// Disconnects remote client `client` through its stack, and steps until the server despawns
    /// its link to it: the client's session closes, and its close ends the server's.
    pub fn disconnect(&mut self, client: usize) {
        let client_entity = self.clients[client].entity;
        let client_of = self.client_of_entity(client);
        self.clients[client]
            .app
            .world_mut()
            .trigger(Disconnect::from(client_entity));
        for _ in 0..WAIT_TICKS {
            if self.server_app.world().get_entity(client_of).is_err() {
                return;
            }
            self.step(1, self.tick_rate.period(), Order::ClientsFirst);
        }
        panic!("the server's link to client {client} still exists after {WAIT_TICKS} ticks");
    }

    /// Starts the server, connects every client and steps until each is connected and ready.
    pub fn init(&mut self) {
        self.connect();
        for _ in 0..WAIT_TICKS {
            if self.waiting_client().is_none() {
                return;
            }
            self.step(1, self.tick_rate.period(), Order::ClientsFirst);
        }
        if let Some(waiting) = self.waiting_client() {
            panic!("{waiting} after {WAIT_TICKS} ticks");
        }
    }

    /// Starts the server and asks every client to connect, without stepping.
    pub fn connect(&mut self) {
        self.start_server();

        for i in 0..self.clients.len() {
            let now = self.instant(AppId::Client(i));
            self.clients[i]
                .app
                .world_mut()
                .resource_mut::<Time<Real>>()
                .update_with_instant(now);
            let client = &mut self.clients[i];
            client.app.world_mut().trigger(Connect {
                entity: client.entity,
            });
        }
        if let Some(host) = self.host_client_entity {
            self.server_app
                .world_mut()
                .trigger(Connect { entity: host });
        }
    }

    /// The first client that is not connected and ready yet, described with its state.
    fn waiting_client(&self) -> Option<String> {
        let state = |entity: EntityRef<'_>, world: &World| {
            if entity.contains::<Connected>() {
                (!self.stack.is_ready(world)).then_some("connected but not ready")
            } else if entity.contains::<Connecting>() {
                Some("connecting")
            } else if entity.contains::<Disconnected>() {
                Some("disconnected")
            } else {
                Some("not connecting")
            }
        };
        let host = self.host_client_entity.and_then(|host| {
            let world = self.server_app.world();
            state(world.entity(host), world).map(|state| format!("the host client is {state}"))
        });
        host.or_else(|| {
            (0..self.clients.len()).find_map(|i| {
                let world = self.clients[i].app.world();
                state(world.entity(self.clients[i].entity), world)
                    .map(|state| format!("client {i} is {state}"))
            })
        })
    }

    /// Finalizes and starts the server once.
    pub fn start_server(&mut self) {
        if self.server_started {
            return;
        }
        if matches!(
            self.server_app.plugins_state(),
            PluginsState::Ready | PluginsState::Adding
        ) {
            self.server_app.finish();
            self.server_app.cleanup();
        }
        // Initialize Real time (needed only for the first TimeSystem run)
        let now = self.instant(AppId::Server);
        self.server_app
            .world_mut()
            .resource_mut::<Time<Real>>()
            .update_with_instant(now);
        // The server's endpoint links at its `LinkStart`, and `RawServer` starts once linked: a
        // host server starts before its host client connects.
        self.server_app.world_mut().trigger(Start {
            entity: self.server_entity,
        });
        self.server_app.world_mut().flush();
        self.server_started = true;
    }

    /// Runs `n` steps of `interval`, every app once a step in `order`, unless it is stalled.
    pub fn step(&mut self, n: usize, interval: Duration, order: Order) {
        for _ in 0..n {
            self.advance_time(interval);
            let clients = self.clients.len();
            for slot in 0..=clients {
                let app = match order {
                    Order::ClientsFirst if slot == clients => AppId::Server,
                    Order::ClientsFirst => AppId::Client(slot),
                    Order::ServerFirst if slot == 0 => AppId::Server,
                    Order::ServerFirst => AppId::Client(slot - 1),
                };
                if !self.schedule(app).is_stalled(self.now) {
                    self.update(app);
                    let now = self.now;
                    self.schedule_mut(app).ran_at(now);
                }
            }
        }
    }

    /// Runs `n` frame intervals, clients first.
    pub fn frame_step(&mut self, n: usize) {
        self.step(n, self.frame_interval, Order::ClientsFirst);
    }

    /// Runs `run` on client `client`'s world in its next `FixedUpdate`, as a game spawns from its
    /// simulation, where the tick is the one both peers agree on, and returns what it returned:
    /// the next frame runs it.
    ///
    /// # Panics
    ///
    /// When the frame runs no `FixedUpdate` on the client, as when its schedule stalls it.
    pub fn client_fixed_update<R: Send + 'static>(
        &mut self,
        client: usize,
        run: impl FnOnce(&mut World) -> R + Send + 'static,
    ) -> R {
        let run = Mutex::new(Some(run));
        let out = Arc::new(Mutex::new(None));
        let sink = Arc::clone(&out);
        self.clients[client]
            .app
            .add_systems(FixedUpdate, move |world: &mut World| {
                if let Some(run) = run.lock().unwrap().take() {
                    *sink.lock().unwrap() = Some(run(world));
                }
            });
        self.frame_step(1);
        out.lock()
            .unwrap()
            .take()
            .expect("the client's frame runs a FixedUpdate")
    }

    fn update(&mut self, app: AppId) {
        match app {
            AppId::Server => error_span!("server").in_scope(|| self.server_app.update()),
            AppId::Client(i) => error_span!("client", ?i).in_scope(|| self.clients[i].app.update()),
        }
    }

    /// Moves the stepper's time and every app's clock by `duration`, each at its drift, without
    /// running a frame.
    pub fn advance_time(&mut self, duration: Duration) {
        self.now += duration;
        self.network.set_time(self.now);
        self.server_schedule.advance_to(self.now);
        let server_now = self.instant(AppId::Server);
        self.server_app
            .insert_resource(TimeUpdateStrategy::ManualInstant(server_now));
        for i in 0..self.clients.len() {
            self.clients[i].schedule.advance_to(self.now);
            let client_now = self.instant(AppId::Client(i));
            self.clients[i]
                .app
                .insert_resource(TimeUpdateStrategy::ManualInstant(client_now));
        }
    }

    /// Runs every app on its own frame interval for `duration`, frame by frame in the order their
    /// frames fall; frames that fall together run clients first, then the server, as
    /// `frame_step` does.
    pub fn run_for(&mut self, duration: Duration) {
        let end = self.now + duration;
        loop {
            let apps = (0..self.clients.len())
                .map(AppId::Client)
                .chain(core::iter::once(AppId::Server));
            let Some((at, app)) = apps
                .map(|app| {
                    let schedule = self.schedule(app);
                    let at = schedule.next_frame.max(schedule.stalled_until);
                    (at.max(self.now), app)
                })
                .filter(|(at, _)| *at <= end)
                .min_by_key(|(at, app)| (*at, matches!(app, AppId::Server)))
            else {
                break;
            };
            self.advance_time(at - self.now);
            self.schedule_mut(app).ran_at(at);
            self.update(app);
        }
        self.advance_time(end - self.now);
    }

    /// Sets the time between `app`'s frames in `run_for`.
    pub fn set_frame_interval(&mut self, app: AppId, interval: Duration) {
        assert!(interval > Duration::ZERO, "a frame interval is positive");
        self.schedule_mut(app).frame_interval = interval;
    }

    /// Makes `app`'s clock run `drift_ppm` parts per million faster than the stepper's from now,
    /// or slower when negative.
    pub fn set_clock_drift(&mut self, app: AppId, drift_ppm: i32) {
        let now = self.now;
        self.schedule_mut(app).set_drift(drift_ppm, now);
    }

    /// Runs no frame of `app` for `duration` from now, as a debugger, a GC pause or a phone in
    /// the background holds it, while its clock and the other apps go on.
    pub fn stall(&mut self, app: AppId, duration: Duration) {
        let until = self.now + duration;
        let schedule = self.schedule_mut(app);
        schedule.stalled_until = schedule.stalled_until.max(until);
    }

    pub fn flush(&mut self) {
        for client in &mut self.clients {
            client.app.world_mut().flush();
        }
        self.server_app.world_mut().flush();
    }

    /// The OS-offset instant that `app`'s clock reads now.
    fn instant(&self, app: AppId) -> Instant {
        self.origin + self.schedule(app).local
    }

    fn schedule(&self, app: AppId) -> &AppSchedule {
        match app {
            AppId::Server => &self.server_schedule,
            AppId::Client(i) => &self.clients[i].schedule,
        }
    }

    fn schedule_mut(&mut self, app: AppId) -> &mut AppSchedule {
        match app {
            AppId::Server => &mut self.server_schedule,
            AppId::Client(i) => &mut self.clients[i].schedule,
        }
    }
}

impl<S> Stepper<S> {
    pub const fn stack(&self) -> &S {
        &self.stack
    }

    pub const fn server_app(&self) -> &App {
        &self.server_app
    }

    pub const fn server_app_mut(&mut self) -> &mut App {
        &mut self.server_app
    }

    pub fn client_app(&self, client: usize) -> &App {
        &self.clients[client].app
    }

    pub fn client_app_mut(&mut self, client: usize) -> &mut App {
        &mut self.clients[client].app
    }

    /// Every app: each remote client's, then the server's.
    pub fn apps_mut(&mut self) -> impl Iterator<Item = &mut App> {
        self.clients
            .iter_mut()
            .map(|client| &mut client.app)
            .chain(core::iter::once(&mut self.server_app))
    }

    /// The number of remote clients.
    pub const fn client_count(&self) -> usize {
        self.clients.len()
    }

    pub const fn server_entity(&self) -> Entity {
        self.server_entity
    }

    pub fn client_entity(&self, client: usize) -> Entity {
        self.clients[client].entity
    }

    /// The server's link to remote client `client`: the one of its session that has not
    /// unlinked, else its last.
    pub fn client_of_entity(&self, client: usize) -> Entity {
        let address = self.clients[client].address;
        let world = self.server_app.world();
        let mut links = world
            .iter_entities()
            .filter(EntityRef::contains::<LinkOf>)
            .filter(|link| link.get::<PeerAddr>().is_some_and(|peer| peer.0 == address));
        let mut last = None;
        for link in links.by_ref() {
            if !link.contains::<Unlinked>() {
                return link.id();
            }
            last = Some(link.id());
        }
        last.unwrap_or_else(|| panic!("the server has no link to client {client} yet"))
    }

    pub const fn host_client_entity(&self) -> Entity {
        self.host_client_entity
            .expect("the stepper has a host client")
    }

    pub const fn frame_interval(&self) -> Duration {
        self.frame_interval
    }

    pub const fn tick_rate(&self) -> TickRate {
        self.tick_rate
    }

    /// The stepper's virtual time, which is also its network's wire time.
    pub const fn now(&self) -> Duration {
        self.now
    }

    /// The network every remote client links over, for its phases.
    pub const fn network(&self) -> &EmulatedNetwork {
        &self.network
    }

    pub fn client_tick(&self, client: usize) -> NetTick {
        self.clients[client]
            .app
            .world()
            .resource::<LocalTimeline>()
            .tick()
    }

    pub fn server_tick(&self) -> NetTick {
        self.server_app.world().resource::<LocalTimeline>().tick()
    }

    pub fn server(&self) -> EntityRef<'_> {
        self.server_app.world().entity(self.server_entity)
    }

    pub fn server_mut(&mut self) -> EntityWorldMut<'_> {
        self.server_app.world_mut().entity_mut(self.server_entity)
    }

    pub fn host_client(&self) -> EntityRef<'_> {
        self.server_app.world().entity(self.host_client_entity())
    }

    pub fn host_client_mut(&mut self) -> EntityWorldMut<'_> {
        let host = self.host_client_entity();
        self.server_app.world_mut().entity_mut(host)
    }

    pub fn client(&self, client: usize) -> EntityRef<'_> {
        let client = &self.clients[client];
        client.app.world().entity(client.entity)
    }

    pub fn client_mut(&mut self, client: usize) -> EntityWorldMut<'_> {
        let client = &mut self.clients[client];
        client.app.world_mut().entity_mut(client.entity)
    }

    pub fn client_of(&self, client: usize) -> EntityRef<'_> {
        self.server_app
            .world()
            .entity(self.client_of_entity(client))
    }

    pub fn client_of_mut(&mut self, client: usize) -> EntityWorldMut<'_> {
        let client_of = self.client_of_entity(client);
        self.server_app.world_mut().entity_mut(client_of)
    }

    /// The simulated link of remote client `client`.
    pub fn client_link(&self, client: usize) -> EmulatedLinkId {
        self.clients[client].wire
    }
}

#[cfg(test)]
mod tests;
