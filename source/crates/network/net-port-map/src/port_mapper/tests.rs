use std::net::UdpSocket;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use super::*;
use crate::gateway::MappedBy;
use crate::gateway::fake_gateway::FakeGateway;
use crate::nat_pmp_map::NatPmpMap;
use crate::pcp_map::PcpMap;
use crate::upnp_gateway::fake_router::{Answer, FakeRouter};

const EXTERNAL: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 50_000);
/// The test server's port and name.
pub(crate) const REQUEST: MapRequest = MapRequest {
    internal_port: 4433,
    name: "campfire-test",
};
/// The production times, with tries and waits short enough for a test.
const TIMES: MapTimes = MapTimes {
    tries: Tries {
        first: Duration::from_millis(20),
        count: 2,
    },
    search: Duration::from_millis(200),
    delete: Duration::from_millis(100),
    ..MapTimes::DEFAULT
};

/// An address of loopback no server listens on: a port bound and let go.
fn closed() -> SocketAddrV4 {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let SocketAddr::V4(address) = socket.local_addr().unwrap() else {
        unreachable!()
    };
    address
}

/// When a mapping of `TIMES`' 7,200 s renews by the draw of seed 0's 3rd, 4th, 5th or 6th word:
/// its low 16 bits d, 0x454F = 17,743, 0x81EC = 33,260, 0x749B = 29,851 or 0xA2EA = 41,706, give
/// 7,200 s × (4 × 65,535 + d) / (8 × 65,535), 16,792,980, 17,724,000, 17,519,460 or 18,230,760
/// over 4,369 s, to the nanosecond below.
const BY_WORD_3: Duration = Duration::from_nanos(3_843_666_742_961);
const BY_WORD_4: Duration = Duration::from_nanos(4_056_763_561_455);
const BY_WORD_5: Duration = Duration::from_nanos(4_009_947_356_374);
const BY_WORD_6: Duration = Duration::from_nanos(4_172_753_490_501);

/// A mapper of `REQUEST` whose draws come from seed 0, `SplitMix64`'s reference sequence: each
/// mapping anew by PCP or NAT-PMP takes two words for its nonce, and each renewal one.
fn seeded(routes: Routes, times: MapTimes) -> PortMapper {
    PortMapper {
        random: Random::seeded(0),
        ..PortMapper::new(REQUEST, routes, times)
    }
}

/// A turn with a stop that never comes.
fn turn(mapper: &mut PortMapper) -> Duration {
    mapper.turn(&AtomicBool::new(false))
}

fn mapping(mapper: &PortMapper) -> Option<Mapping> {
    mapper.held.as_ref().map(|held| held.mapping)
}

#[test]
fn a_pcp_mapping_is_renewed_within_its_lifetime_and_deleted_at_the_end() {
    let gateway = FakeGateway::start(|request| vec![FakeGateway::pcp_reply(request, 0, EXTERNAL)]);
    let routes = Routes::At {
        pcp: gateway.address,
        ssdp: SocketAddr::V4(closed()),
        announce: closed(),
    };
    let mut mapper = seeded(routes, TIMES);
    assert_eq!(turn(&mut mapper), BY_WORD_3);
    let first = mapping(&mapper).unwrap();
    assert_eq!(first.external(), Some(EXTERNAL));
    assert_eq!(first.by, MappedBy::Pcp);
    assert_eq!(turn(&mut mapper), BY_WORD_4);
    assert_eq!(mapping(&mapper), Some(first));
    mapper.end();
    assert_eq!(mapping(&mapper), None);
    // The nonce of seed 0's first two words, as `Random` gives them.
    let nonce = MapNonce::random(&mut Random::seeded(0));
    let request = |external, lifetime| {
        PcpMap {
            nonce,
            client: Ipv4Addr::LOCALHOST,
            internal_port: 4433,
            external,
            lifetime,
        }
        .encode()
        .to_vec()
    };
    let unspecified = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);
    assert_eq!(
        gateway.requests(),
        [
            request(unspecified, TIMES.lifetime),
            request(EXTERNAL, TIMES.lifetime),
            request(unspecified, Duration::ZERO),
        ]
    );
}

#[test]
fn a_renewal_the_gateway_refuses_maps_the_port_anew_in_the_same_turn() {
    // The gateway grants, refuses the renewal, NOT_AUTHORIZED, and grants again.
    let answered = Arc::new(AtomicUsize::new(0));
    let gateway = {
        let answered = Arc::clone(&answered);
        FakeGateway::start(move |request| {
            let result = if answered.fetch_add(1, Ordering::Relaxed) == 1 {
                2
            } else {
                0
            };
            vec![FakeGateway::pcp_reply(request, result, EXTERNAL)]
        })
    };
    let routes = Routes::At {
        pcp: gateway.address,
        ssdp: SocketAddr::V4(closed()),
        announce: closed(),
    };
    let mut mapper = seeded(routes, TIMES);
    assert_eq!(turn(&mut mapper), BY_WORD_3);
    // The mapping anew draws the 4th and 5th words for its own nonce, and its renewal the 6th.
    assert_eq!(turn(&mut mapper), BY_WORD_6);
    let nonces: Vec<[u8; 12]> = gateway
        .requests()
        .iter()
        .map(|request| request[24..36].try_into().unwrap())
        .collect();
    // The bytes of 0xE220A8397B1DCDAF, then the low 4 of 0x6E789E6AA1B965F4; the request anew,
    // of 0xF88BB8A8724C81EC and 0x1B39896A51A8749B.
    let first = [175, 205, 29, 123, 57, 168, 32, 226, 244, 101, 185, 161];
    let anew = [236, 129, 76, 114, 168, 184, 139, 248, 155, 116, 168, 81];
    assert_eq!(nonces, [first, first, anew]);
}

#[test]
fn a_gateway_with_no_pcp_server_gives_way_to_a_upnp_router() {
    let fake = FakeRouter::start(|_| Answer::Done);
    let routes = Routes::At {
        pcp: closed(),
        ssdp: fake.ssdp(),
        announce: closed(),
    };
    // The gateway's client draws a nonce before PCP finds no server.
    let mut mapper = seeded(routes, TIMES);
    assert_eq!(turn(&mut mapper), BY_WORD_3);
    assert_eq!(
        mapping(&mapper),
        Some(Mapping {
            external_port: 4433,
            address: Some(FakeRouter::EXTERNAL_IP),
            lifetime: TIMES.lifetime,
            by: MappedBy::Upnp,
        })
    );
    mapper.end();
    let actions: Vec<String> = fake.calls().into_iter().map(|call| call.action).collect();
    assert_eq!(
        actions,
        [
            "AddPortMapping",
            "GetExternalIPAddress",
            "DeletePortMapping"
        ]
    );
}

#[test]
fn a_grant_whose_address_does_not_read_is_held_and_deleted_at_the_end() {
    // A NAT-PMP gateway, as PCP's version 2 is unsupported, that grants port 50,000 for 3,600 s
    // and never answers the request of its address outside.
    let gateway = FakeGateway::start(|request| match request[..2] {
        [2, _] => vec![vec![0, 0, 0, 1, 0, 0, 0, 5]],
        [0, 1] => {
            let mut reply = vec![0, 129, 0, 0, 0, 0, 0x03, 0xe8];
            reply.extend(&request[4..6]);
            reply.extend([0xc3, 0x50, 0, 0, 0x0e, 0x10]);
            vec![reply]
        }
        _ => vec![],
    });
    let routes = Routes::At {
        pcp: gateway.address,
        ssdp: SocketAddr::V4(closed()),
        announce: closed(),
    };
    let mut mapper = seeded(routes, TIMES);
    // The renewal of 3,600 s by the 3rd word's 17,743: 3,600 s × (4 × 65,535 + 17,743) /
    // (8 × 65,535) = 8,396,490 / 4,369 s.
    assert_eq!(turn(&mut mapper), Duration::from_nanos(1_921_833_371_480));
    assert_eq!(
        mapping(&mapper),
        Some(Mapping {
            external_port: 50_000,
            address: None,
            lifetime: Duration::from_secs(3600),
            by: MappedBy::NatPmp,
        })
    );
    mapper.end();
    assert_eq!(mapping(&mapper), None);
    let deletion = NatPmpMap {
        internal_port: 4433,
        external_port: 0,
        lifetime: Duration::ZERO,
    };
    let requests = gateway.requests();
    // PCP, the mapping, the address asked TIMES' two tries, and the deletion.
    let kinds: Vec<[u8; 2]> = requests
        .iter()
        .map(|request| [request[0], request[1]])
        .collect();
    assert_eq!(kinds, [[2, 1], [0, 1], [0, 0], [0, 0], [0, 1]]);
    assert_eq!(requests[4], deletion.encode().to_vec());
}

#[test]
fn a_stop_ends_a_turn_before_its_next_request() {
    let gateway = FakeGateway::start(|request| vec![FakeGateway::pcp_reply(request, 0, EXTERNAL)]);
    let router = FakeRouter::start(|_| Answer::Done);
    let routes = Routes::At {
        pcp: gateway.address,
        ssdp: router.ssdp(),
        announce: closed(),
    };
    let mut mapper = seeded(routes, TIMES);
    assert_eq!(mapper.turn(&AtomicBool::new(true)), Duration::ZERO);
    assert_eq!(mapping(&mapper), None);
    assert!(!mapper.told);
    assert_eq!(gateway.requests(), Vec::<Vec<u8>>::new());
    assert_eq!(router.calls().len(), 0);

    // A mapping held when the map stops is kept for the end, and its renewal sends nothing. The
    // stopped turn drew its nonce, so this one draws the 3rd and 4th words, and its renewal the
    // 5th.
    assert_eq!(turn(&mut mapper), BY_WORD_5);
    let held = mapping(&mapper);
    assert_eq!(mapper.turn(&AtomicBool::new(true)), Duration::ZERO);
    assert_eq!(mapping(&mapper), held);
    assert_eq!(gateway.requests().len(), 1);
}

#[test]
fn a_port_no_router_maps_waits_its_retry_and_a_router_that_announces_itself_maps_it_at_once() {
    // A socket that reads the search and never answers.
    let silent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let routes = Routes::At {
        pcp: closed(),
        ssdp: silent.local_addr().unwrap(),
        announce: closed(),
    };
    let mut mapper = seeded(routes, TIMES);
    assert_eq!(turn(&mut mapper), TIMES.retry);
    assert!(mapper.told);
    assert_eq!(mapping(&mapper), None);
    // A router announces itself, as a host firewall drops its answer to the search: it starts a
    // turn at once, which maps the port through it; its next announcement starts none.
    let router = FakeRouter::start(|_| Answer::Done);
    let from = SocketAddr::from((Ipv4Addr::LOCALHOST, 41_952));
    assert!(!mapper.hear(b"M-SEARCH * HTTP/1.1\r\n\r\n", from));
    assert!(mapper.hear(&router.announcement(), from));
    // Each turn's PCP drew a nonce, so the renewal draws the 5th word.
    assert_eq!(turn(&mut mapper), BY_WORD_5);
    let by_router = SocketAddrV4::new(FakeRouter::EXTERNAL_IP, 4433);
    assert_eq!(
        mapping(&mapper).and_then(|mapping| mapping.external()),
        Some(by_router)
    );
    assert!(!mapper.hear(&router.announcement(), from));
    mapper.end();
    let calls = router.calls();
    let actions: Vec<&str> = calls.iter().map(|call| call.action.as_str()).collect();
    assert_eq!(
        actions,
        [
            "AddPortMapping",
            "GetExternalIPAddress",
            "DeletePortMapping"
        ]
    );
}

#[test]
fn a_pcp_refusal_is_asked_again_once_its_lifetime_ends() {
    // NOT_AUTHORIZED for the 7,000 s an earlier run's mapping has left, NO_RESOURCES for 30 s,
    // and NETWORK_FAILURE for 1 s, which the floor of 4 s lifts. A refusal past the 7,200 s a
    // mapping lasts, 7,201 s or about 136 years, waits those 7,200 s.
    for (result, lasts, wait) in [
        (2, 7000, Duration::from_secs(7000)),
        (2, 7201, TIMES.lifetime),
        (2, u32::MAX, TIMES.lifetime),
        (8, 30, Duration::from_secs(30)),
        (7, 1, RENEWAL_MIN),
    ] {
        let gateway = FakeGateway::start(move |request| {
            let mut reply = FakeGateway::pcp_reply(request, result, EXTERNAL);
            reply[4..8].copy_from_slice(&u32::to_be_bytes(lasts));
            vec![reply]
        });
        let silent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let routes = Routes::At {
            pcp: gateway.address,
            ssdp: silent.local_addr().unwrap(),
            announce: closed(),
        };
        // No router answers the search, so it waits its whole time, here a short one.
        let times = MapTimes {
            search: Duration::from_millis(50),
            ..TIMES
        };
        let mut mapper = PortMapper::new(REQUEST, routes, times);
        assert_eq!(turn(&mut mapper), wait, "result {result}");
        assert_eq!(mapping(&mapper), None);
    }
}

#[test]
fn a_renewal_falls_between_half_and_five_eighths_of_the_lifetime() {
    let hour = Duration::from_secs(3600);
    assert_eq!(PortMapper::renewal(hour, 0), Duration::from_mins(30));
    assert_eq!(
        PortMapper::renewal(hour, u16::MAX),
        Duration::from_secs(2250)
    );
    // Half the draws, 32,767.5 of 65,535, is 9/16 of the lifetime, 2,025 s; 32,768 is past it by
    // 450 s / 65,535 of a draw, about 3.4 ms.
    let middle = PortMapper::renewal(hour, 32_768);
    assert!(middle > Duration::from_secs(2025) && middle < Duration::from_millis(2_025_004));
    // A lifetime of 6 s renews at 3 s at the earliest, which the 4 s floor lifts.
    assert_eq!(PortMapper::renewal(Duration::from_secs(6), 0), RENEWAL_MIN);
}

#[test]
fn an_external_address_is_public_unless_a_nat_or_the_host_holds_it() {
    for (address, public) in [
        ([203, 0, 113, 7], true),
        ([8, 8, 8, 8], true),
        // RFC 1918: 10/8, 172.16/12, 192.168/16, each at both ends.
        ([10, 0, 0, 0], false),
        ([10, 255, 255, 255], false),
        ([172, 16, 0, 0], false),
        ([172, 31, 255, 255], false),
        ([172, 32, 0, 0], true),
        ([192, 168, 100, 120], false),
        // RFC 6598: 100.64/10, 100.64.0.0 to 100.127.255.255.
        ([100, 63, 255, 255], true),
        ([100, 64, 0, 0], false),
        ([100, 127, 255, 255], false),
        ([100, 128, 0, 0], true),
        ([169, 254, 1, 1], false),
        ([127, 0, 0, 1], false),
        ([0, 0, 0, 0], false),
    ] {
        assert_eq!(
            PortMapper::public(Ipv4Addr::from(address)),
            public,
            "{address:?}"
        );
    }
}
