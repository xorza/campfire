use super::*;
use crate::gateway::fake_gateway::FakeGateway;
use crate::nat_pmp_map::NatPmpRefusal;
use crate::pcp_map::PcpRefusal;

const EXTERNAL: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 50_000);
const LIFETIME: Duration = Duration::from_secs(7200);
/// Tries short enough for a test: 20, 40 and 80 ms.
const TRIES: Tries = Tries {
    first: Duration::from_millis(20),
    count: 3,
};

#[test]
fn a_pcp_mapping_is_renewed_with_its_nonce_at_its_address_and_deleted() {
    let gateway = FakeGateway::start(|request| vec![FakeGateway::pcp_reply(request, 0, EXTERNAL)]);
    let mut client = Gateway::connect(gateway.address).unwrap();
    let mapping = client.map(4433, None, LIFETIME, TRIES).unwrap();
    let MappedBy::Pcp(nonce) = mapping.by else {
        panic!("a PCP gateway maps by PCP");
    };
    assert_eq!(
        mapping,
        Mapping {
            external: EXTERNAL,
            lifetime: LIFETIME,
            by: MappedBy::Pcp(nonce),
        }
    );
    let renewed = client.map(4433, Some(&mapping), LIFETIME, TRIES).unwrap();
    assert_eq!(renewed, mapping);
    client
        .delete(4433, &mapping, Duration::from_millis(100))
        .unwrap();

    let host = Ipv4Addr::LOCALHOST;
    let request = |external, lifetime| PcpMap {
        nonce,
        client: host,
        internal_port: 4433,
        external,
        lifetime,
    };
    // The first asks for no address, the renewal for the one granted, and the deletion for none,
    // a lifetime of 0; all three name the mapping by its one nonce.
    assert_eq!(
        gateway.requests(),
        [
            request(NO_EXTERNAL, LIFETIME).encode().to_vec(),
            request(EXTERNAL, LIFETIME).encode().to_vec(),
            request(NO_EXTERNAL, Duration::ZERO).encode().to_vec(),
        ]
    );
}

#[test]
fn a_gateway_that_answers_pcp_with_version_0_is_asked_by_nat_pmp_from_then_on() {
    let gateway = FakeGateway::start(|request| {
        vec![match request[..2] {
            // NAT-PMP's Unsupported Version, epoch 5.
            [2, _] => vec![0, 0, 0, 1, 0, 0, 0, 5],
            // A mapping of the request's internal port to 50,000 for 3,600 s, epoch 1,000 s.
            [0, 1] => {
                let mut reply = vec![0, 129, 0, 0, 0, 0, 0x03, 0xe8];
                reply.extend(&request[4..6]);
                reply.extend([0xc3, 0x50, 0, 0, 0x0e, 0x10]);
                reply
            }
            [0, 0] => vec![0, 128, 0, 0, 0, 0, 0x03, 0xe8, 203, 0, 113, 7],
            _ => unreachable!("the client sends PCP and NAT-PMP alone"),
        }]
    });
    let mut client = Gateway::connect(gateway.address).unwrap();
    let mapping = client.map(4433, None, LIFETIME, TRIES).unwrap();
    assert_eq!(
        mapping,
        Mapping {
            external: EXTERNAL,
            lifetime: Duration::from_secs(3600),
            by: MappedBy::NatPmp,
        }
    );
    assert_eq!(
        client.map(4433, Some(&mapping), LIFETIME, TRIES).unwrap(),
        mapping
    );
    client
        .delete(4433, &mapping, Duration::from_millis(100))
        .unwrap();
    let requests = gateway.requests();
    let kinds: Vec<[u8; 2]> = requests
        .iter()
        .map(|request| [request[0], request[1]])
        .collect();
    assert_eq!(kinds, [[2, 1], [0, 1], [0, 0], [0, 1], [0, 0], [0, 1]]);
    // The first mapping suggests no port, its renewal the one granted, and the deletion asks for
    // none for no time.
    let map = |external_port, lifetime| {
        NatPmpMap {
            internal_port: 4433,
            external_port,
            lifetime,
        }
        .encode()
        .to_vec()
    };
    assert_eq!(requests[1], map(0, LIFETIME));
    assert_eq!(requests[3], map(50_000, LIFETIME));
    assert_eq!(requests[5], map(0, Duration::ZERO));
}

#[test]
fn a_silent_gateway_is_asked_each_try_and_given_up() {
    let gateway = FakeGateway::start(|_| vec![]);
    let mut client = Gateway::connect(gateway.address).unwrap();
    let start = Instant::now();
    let error = client.map(4433, None, LIFETIME, TRIES).unwrap_err();
    assert!(matches!(error, MapError::NoAnswer), "{error:?}");
    // 20 + 40 + 80 ms of waits, each after its own request.
    assert!(start.elapsed() >= Duration::from_millis(140));
    assert_eq!(gateway.requests_once(3).len(), 3);
}

#[test]
fn a_reply_to_another_request_is_passed_over_and_a_refusal_is_the_answer() {
    let gateway = FakeGateway::start(|request| {
        let mut stray = FakeGateway::pcp_reply(request, 0, EXTERNAL);
        stray[24] ^= 1;
        let mut refusal = FakeGateway::pcp_reply(request, 2, EXTERNAL);
        refusal[4..8].copy_from_slice(&30_u32.to_be_bytes());
        vec![vec![9, 9, 9], stray, refusal]
    });
    let mut client = Gateway::connect(gateway.address).unwrap();
    let error = client.map(4433, None, LIFETIME, TRIES).unwrap_err();
    assert!(
        matches!(
            error,
            MapError::PcpRefused {
                refusal: PcpRefusal::NotAuthorized,
                lasts,
            } if lasts == Duration::from_secs(30)
        ),
        "{error:?}"
    );
    assert_eq!(gateway.requests().len(), 1);

    let gateway = FakeGateway::start(|request| {
        vec![match request[0] {
            2 => vec![0, 0, 0, 1, 0, 0, 0, 5],
            _ => vec![0, 129, 0, 2, 0, 0, 0, 5],
        }]
    });
    let mut client = Gateway::connect(gateway.address).unwrap();
    let error = client.map(4433, None, LIFETIME, TRIES).unwrap_err();
    assert!(
        matches!(error, MapError::NatPmpRefused(NatPmpRefusal::NotAuthorized)),
        "{error:?}"
    );
}

#[test]
fn a_gateway_port_no_server_listens_on_is_told_at_once() {
    // A port that was bound and let go: the OS answers the request with ICMP.
    let closed = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let SocketAddr::V4(address) = closed.local_addr().unwrap() else {
        unreachable!()
    };
    drop(closed);
    let mut client = Gateway::connect(address).unwrap();
    let start = Instant::now();
    let error = client.map(4433, None, LIFETIME, TRIES).unwrap_err();
    assert!(matches!(error, MapError::NoServer), "{error:?}");
    assert!(start.elapsed() < TRIES.first);
}
