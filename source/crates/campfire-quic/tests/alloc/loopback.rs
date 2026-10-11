use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};

use bevy_app::App;
use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{Schedules, SingleThreadedExecutor};
use bevy_ecs::world::Mut;
use campfire_common::TlsKey;
use campfire_quic::{LINK_MIN_MTU, LinkTimes, QuicEndpoint, QuicPlugin};
use lightyear::link::{Link, LinkMtu, LinkStart, Linked};
use zeroize::Zeroizing;

/// The longest a fixture waits for loopback, inside a test's bound of 1 s.
const PATIENCE: Duration = Duration::from_millis(800);

/// Every packet a fixture sends: the largest a link carries on QUIC's minimum path, static, so
/// its sender allocates nothing to send it.
pub(crate) static PACKET: [u8; LINK_MIN_MTU] = [0xa5; LINK_MIN_MTU];

/// The secret of the fixtures' server key.
pub(crate) fn secret() -> Zeroizing<[u8; 32]> {
    Zeroizing::new([7; 32])
}

/// An app with the QUIC plugin, whose schedules run each system on the app's own thread, where
/// a window counts it.
pub(crate) fn app() -> App {
    let mut app = App::new();
    app.add_plugins(QuicPlugin);
    for (_, schedule) in app.world_mut().resource_mut::<Schedules>().iter_mut() {
        schedule.set_executor(SingleThreadedExecutor::new());
    }
    app
}

/// Calls `step` until it says it is done, a millisecond apart.
pub(crate) fn until(mut step: impl FnMut() -> bool) {
    let start = Instant::now();
    while !step() {
        assert!(start.elapsed() < PATIENCE, "loopback did not answer");
        thread::sleep(Duration::from_millis(1));
    }
}

/// A client's app, its entity the link and the endpoint.
#[derive(Debug)]
pub(crate) struct ClientApp {
    pub(crate) app: App,
    entity: Entity,
}

impl ClientApp {
    /// A client that connects to the server at `server`, whose key is `key`.
    pub(crate) fn connect(server: SocketAddr, key: &TlsKey) -> ClientApp {
        let quic = QuicEndpoint::client(server, key, LinkTimes::CAMPFIRE).unwrap();
        let mut app = app();
        let entity = app
            .world_mut()
            .spawn((Link::default().with_mtu(LinkMtu::new(LINK_MIN_MTU)), quic))
            .id();
        app.world_mut().trigger(LinkStart { entity });
        ClientApp { app, entity }
    }

    pub(crate) fn is_linked(&self) -> bool {
        self.app.world().get::<Linked>(self.entity).is_some()
    }

    pub(crate) fn link(&mut self) -> Mut<'_, Link> {
        self.app.world_mut().get_mut::<Link>(self.entity).unwrap()
    }
}
