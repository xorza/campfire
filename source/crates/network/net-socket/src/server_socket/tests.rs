use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use std::thread;
use std::time::Instant as OsInstant;

use campfire_net_packets::internals::{CLIENT, PlainHost};
use campfire_net_payload::{PayloadFate, PayloadToken};
use campfire_net_udp::UdpSetup;
use campfire_net_udp::internals::TestLink;

use crate::client_socket::ClientSocket;
use crate::server_socket::ServerSocket;
use crate::udp_io::UdpIo;

/// The longest a test waits for the network on loopback, inside a test's bound of 1 s.
const PATIENCE: Duration = Duration::from_millis(800);

/// A server and a client on loopback sockets, each run in real time by its IO thread.
#[derive(Debug)]
struct Loopback {
    server: PlainHost<UdpIo>,
    client: PlainHost<UdpIo>,
}

impl Loopback {
    fn new(setup: &UdpSetup) -> Self {
        let origin = OsInstant::now();
        let socket = ServerSocket::bind(
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            TestLink::server(setup.clone()),
        )
        .unwrap();
        let address = socket.local_address();
        let mut server = PlainHost::os(socket.endpoint(TestLink::roomy()), origin);
        server.start(server.now());
        let client_socket = ClientSocket::bind(address, TestLink::client(setup.clone())).unwrap();
        let mut client = PlainHost::os(client_socket.endpoint(), origin);
        client.start(client.now());
        Self { server, client }
    }

    /// Runs a frame of each host until `done` holds, within `PATIENCE`.
    fn pump(&mut self, mut done: impl FnMut(&Self) -> bool) {
        let start = OsInstant::now();
        while !done(self) {
            assert!(
                start.elapsed() < PATIENCE,
                "the condition held within the patience"
            );
            self.frames();
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn frames(&mut self) {
        self.client.frame(self.client.now());
        self.server.frame(self.server.now());
    }

    fn is_linked(&self) -> bool {
        self.client.link(CLIENT).is_some_and(|link| link.linked)
            && self.server.linked().count() == 1
    }
}

/// A client links over loopback sockets, and its payload crosses and is delivered.
#[test]
fn a_client_links_over_loopback_and_a_payload_crosses() {
    let mut loopback = Loopback::new(&TestLink::setup());
    loopback.pump(Loopback::is_linked);
    loopback.client.queue(CLIENT, 1, b"over loopback");
    loopback.pump(|loopback| {
        loopback
            .client
            .link(CLIENT)
            .unwrap()
            .fates
            .contains(&PayloadFate::Delivered(PayloadToken(1)))
    });
    let link = loopback.server.linked().next().unwrap();
    assert_eq!(
        loopback.server.link(link).unwrap().opened,
        [b"over loopback".to_vec()]
    );
}

/// Hosts that run no frame for twice the idle timeout keep their link: their IO threads send the
/// keep-alives and acknowledge them.
#[test]
fn a_stalled_app_keeps_its_link() {
    let mut loopback = Loopback::new(&TestLink::short());
    loopback.pump(Loopback::is_linked);
    thread::sleep(Duration::from_millis(400));
    loopback.frames();
    let client = loopback.client.link(CLIENT).unwrap();
    assert_eq!(client.ended, None);
    assert!(client.linked);
}

/// A second socket on a port the first holds is an error that names the address.
#[test]
fn a_port_in_use_is_an_error_that_names_its_address() {
    let first = ServerSocket::bind(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        TestLink::server(TestLink::setup()),
    )
    .unwrap();
    let address = first.local_address();
    let error = ServerSocket::bind(address, TestLink::server(TestLink::setup())).unwrap_err();
    assert_eq!(error.address, address);
}
