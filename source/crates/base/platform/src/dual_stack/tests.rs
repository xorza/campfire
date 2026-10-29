use std::net::{Ipv4Addr, Ipv6Addr};

use super::*;

#[test]
fn a_socket_on_the_ipv6_wildcard_takes_an_ipv4_peer() {
    // Bound to [::], a socket takes a datagram from 127.0.0.1 at the IPv4-mapped address of its
    // sender; bound to 127.0.0.1, one takes IPv4 as before.
    let wildcard = DualStack::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))).unwrap();
    let port = wildcard.local_addr().unwrap().port();
    let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    sender.send_to(b"hi", (Ipv4Addr::LOCALHOST, port)).unwrap();
    let mut buffer = [0; 8];
    let (len, from) = wildcard.recv_from(&mut buffer).unwrap();
    assert_eq!(&buffer[..len], b"hi");
    assert_eq!(
        from,
        SocketAddr::from((
            Ipv4Addr::LOCALHOST.to_ipv6_mapped(),
            sender.local_addr().unwrap().port()
        ))
    );
    let ipv4 = DualStack::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    assert!(ipv4.local_addr().unwrap().is_ipv4());
}
