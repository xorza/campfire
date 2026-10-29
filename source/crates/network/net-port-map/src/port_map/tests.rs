use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use super::*;
use crate::gateway::Tries;
use crate::gateway::fake_gateway::FakeGateway;
use crate::port_mapper::tests::REQUEST;
use crate::upnp_gateway::fake_router::{Answer, FakeRouter};

#[test]
fn a_dropped_map_wakes_its_thread_which_deletes_the_mapping() {
    let external = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 50_000);
    let gateway =
        FakeGateway::start(move |request| vec![FakeGateway::pcp_reply(request, 0, external)]);
    let silent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let routes = Routes::At {
        pcp: gateway.address,
        ssdp: silent.local_addr().unwrap(),
        announce: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0),
    };
    let times = MapTimes {
        tries: Tries {
            first: Duration::from_millis(20),
            count: 2,
        },
        delete: Duration::from_millis(100),
        ..MapTimes::DEFAULT
    };
    let map = PortMap::of(PortMapper::new(REQUEST, routes, times)).unwrap();
    let start = Instant::now();
    while gateway.requests().is_empty() {
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "the map did not ask"
        );
        thread::sleep(Duration::from_millis(1));
    }
    // The thread waits an hour to renew; the drop ends it at once, with the deletion.
    let start = Instant::now();
    drop(map);
    assert!(start.elapsed() < Duration::from_millis(200));
    let requests = gateway.requests();
    assert_eq!(requests.len(), 2);
    // The deletion: a lifetime of 0.
    assert_eq!(requests[1][4..8], [0; 4]);
}

#[test]
fn a_router_forwards_to_a_socket_that_takes_ipv4_and_not_loopback() {
    for (ip, forwardable) in [
        (IpAddr::V4(Ipv4Addr::UNSPECIFIED), true),
        (IpAddr::V4(Ipv4Addr::new(192, 168, 0, 5)), true),
        (IpAddr::V4(Ipv4Addr::LOCALHOST), false),
        (IpAddr::V6(Ipv6Addr::UNSPECIFIED), true),
        (IpAddr::V6(Ipv6Addr::LOCALHOST), false),
        (
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
            false,
        ),
    ] {
        assert_eq!(
            PortMap::is_forwardable(SocketAddr::new(ip, 4433)),
            forwardable,
            "{ip}"
        );
    }
}

#[test]
fn an_announcement_cuts_the_wait_after_a_turn_that_mapped_nothing() {
    // No PCP server, and a search no router answers: the turn maps nothing, and waits its
    // retry of 30 minutes, which a router's announcement cuts short.
    let router = FakeRouter::start(|_| Answer::Done);
    let closed = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let SocketAddr::V4(pcp) = closed.local_addr().unwrap() else {
        unreachable!("a socket bound to IPv4 has an IPv4 address");
    };
    drop(closed);
    let silent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let listener = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let SocketAddr::V4(announce) = listener.local_addr().unwrap() else {
        unreachable!("a socket bound to IPv4 has an IPv4 address");
    };
    drop(listener);
    let routes = Routes::At {
        pcp,
        ssdp: silent.local_addr().unwrap(),
        announce,
    };
    let times = MapTimes {
        tries: Tries {
            first: Duration::from_millis(20),
            count: 2,
        },
        search: Duration::from_millis(50),
        delete: Duration::from_millis(100),
        ..MapTimes::DEFAULT
    };
    let map = PortMap::of(PortMapper::new(REQUEST, routes, times)).unwrap();
    let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    sender.send_to(&router.announcement(), announce).unwrap();
    let start = Instant::now();
    while !router
        .calls()
        .iter()
        .any(|call| call.action == "AddPortMapping")
    {
        assert!(
            start.elapsed() < Duration::from_millis(800),
            "the announcement started no turn"
        );
        thread::sleep(Duration::from_millis(1));
    }
    drop(map);
    assert_eq!(router.calls().last().unwrap().action, "DeletePortMapping");
}
