use std::io::ErrorKind;
use std::net::Ipv4Addr;

use super::*;

#[test]
fn a_shared_port_binds_twice_and_an_unshared_socket_not_beside_it() {
    let first = SharedPort::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
    let SocketAddr::V4(address) = first.socket().local_addr().unwrap() else {
        unreachable!("a socket bound to IPv4 has an IPv4 address");
    };
    let second = SharedPort::bind(address).unwrap();
    assert_eq!(
        second.socket().local_addr().unwrap(),
        SocketAddr::V4(address)
    );
    let alone = UdpSocket::bind(address).unwrap_err();
    assert_eq!(alone.kind(), ErrorKind::AddrInUse);
}
