use std::net::UdpSocket;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use super::*;
use crate::upnp_gateway::fake_router::{Answer, Call, FakeRouter};

const LIFETIME: Duration = Duration::from_secs(7200);
const EXTERNAL_IP: Ipv4Addr = FakeRouter::EXTERNAL_IP;
/// The ports seed 0 draws: `0x8000 |` the low 16 bits of SplitMix64's first two words from 0,
/// 0xE220A8397B1DCDAF and 0x6E789E6AA1B965F4, so 0xCDAF and 0xE5F4.
const FIRST_DRAW: u16 = 52_655;
const SECOND_DRAW: u16 = 58_868;

fn search(router: &FakeRouter) -> UpnpGateway {
    UpnpGateway::search(
        router.ssdp(),
        Duration::from_millis(500),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn ask(renewed: Option<&Mapping>) -> Ask<'_> {
    Ask {
        internal_port: 4433,
        name: "campfire-test",
        renewed,
        lifetime: LIFETIME,
    }
}

/// Maps port 4433 for `LIFETIME`, anew or `renewed`, with draws of seed 0 and a stop that never
/// comes.
fn map(gateway: &UpnpGateway, renewed: Option<&Mapping>) -> Result<Mapping, MapError> {
    gateway.map(
        ask(renewed),
        &AtomicBool::new(false),
        &mut Random::seeded(0),
    )
}

/// The port outside of each `AddPortMapping` the router took, in order.
fn added_ports(router: &FakeRouter) -> Vec<String> {
    router
        .calls()
        .iter()
        .filter(|call| call.action == "AddPortMapping")
        .map(|call| call.arg("NewExternalPort").to_owned())
        .collect()
}

#[test]
fn a_router_maps_the_port_at_the_same_port_renews_it_and_deletes_it() {
    let router = FakeRouter::start(|_| Answer::Done);
    let gateway = search(&router);
    let mapping = map(&gateway, None).unwrap();
    assert_eq!(
        mapping,
        Mapping {
            external_port: 4433,
            address: None,
            lifetime: LIFETIME,
            by: MappedBy::Upnp,
        }
    );
    assert_eq!(gateway.address().unwrap(), EXTERNAL_IP);
    assert_eq!(map(&gateway, Some(&mapping)).unwrap(), mapping);
    gateway.delete(&mapping).unwrap();
    let calls = router.calls();
    let actions: Vec<&str> = calls.iter().map(|call| call.action.as_str()).collect();
    assert_eq!(
        actions,
        [
            "AddPortMapping",
            "GetExternalIPAddress",
            "AddPortMapping",
            "DeletePortMapping",
        ]
    );
    let add = &calls[0];
    for (arg, value) in [
        ("NewExternalPort", "4433"),
        ("NewInternalPort", "4433"),
        ("NewInternalClient", "127.0.0.1"),
        ("NewProtocol", "UDP"),
        ("NewLeaseDuration", "7200"),
        ("NewPortMappingDescription", "campfire-test"),
    ] {
        assert_eq!(add.arg(arg), value, "{arg}");
    }
    assert_eq!(calls[3].arg("NewExternalPort"), "4433");
    assert_eq!(calls[3].arg("NewProtocol"), "UDP");
}

#[test]
fn a_router_that_takes_only_permanent_leases_gets_one_at_its_own_port_or_another() {
    // 725, OnlyPermanentLeasesSupported, for any lease but 0; the second router also holds port
    // 4433 for another host, 718, so the map draws another port.
    for held in [false, true] {
        let router = FakeRouter::start(move |call| match call.action.as_str() {
            "AddPortMapping" if held && call.arg("NewExternalPort") == "4433" => Answer::Error(718),
            "AddPortMapping" if call.arg("NewLeaseDuration") != "0" => Answer::Error(725),
            _ => Answer::Done,
        });
        let mapping = map(&search(&router), None).unwrap();
        // The mapping still renews within the lifetime asked for.
        assert_eq!(mapping.lifetime, LIFETIME);
        let adds: Vec<(String, String)> = router
            .calls()
            .iter()
            .filter(|call| call.action == "AddPortMapping")
            .map(|call| {
                (
                    call.arg("NewExternalPort").to_owned(),
                    call.arg("NewLeaseDuration").to_owned(),
                )
            })
            .collect();
        let leases: Vec<&str> = adds.iter().map(|(_, lease)| lease.as_str()).collect();
        if held {
            // 4433 with a lease, which the other host holds; a drawn port with a lease, refused as
            // a lease; and a port drawn again with none.
            assert_eq!(leases, ["7200", "7200", "0"]);
            assert_eq!(adds[1].0, FIRST_DRAW.to_string());
            assert_eq!(adds[2].0, SECOND_DRAW.to_string());
            assert_eq!(mapping.external_port, SECOND_DRAW);
        } else {
            assert_eq!(leases, ["7200", "0"]);
            assert_eq!(mapping.external_port, 4433);
        }
    }
}

#[test]
fn a_port_another_host_holds_is_mapped_at_another_and_its_renewal_fails() {
    // 718, ConflictInMappingEntry, for port 4433 outside.
    let router = FakeRouter::start(|call| match call.action.as_str() {
        "AddPortMapping" if call.arg("NewExternalPort") == "4433" => Answer::Error(718),
        _ => Answer::Done,
    });
    let gateway = search(&router);
    let mapping = map(&gateway, None).unwrap();
    let adds: Vec<Call> = router
        .calls()
        .into_iter()
        .filter(|call| call.action == "AddPortMapping")
        .collect();
    // The router lacks AddAnyPortMapping, so the map asks for a port it draws.
    assert_eq!(adds.len(), 2);
    assert_eq!(adds[1].arg("NewExternalPort"), FIRST_DRAW.to_string());
    assert_eq!(adds[1].arg("NewInternalPort"), "4433");
    assert_eq!(mapping.external_port, FIRST_DRAW);

    let held = Mapping {
        external_port: 4433,
        ..mapping
    };
    let error = map(&gateway, Some(&held)).unwrap_err();
    assert!(
        matches!(
            error,
            MapError::UpnpAdd(SoapError::Refused(CONFLICT_IN_MAPPING_ENTRY))
        ),
        "{error:?}"
    );
}

/// A router with `AddAnyPortMapping` reserves the port: the map asks for a drawn one and takes
/// the one the router names, here the drawn port with its lowest bit flipped.
#[test]
fn a_router_that_offers_add_any_reserves_a_port_of_its_own() {
    let router = FakeRouter::offering_add_any(|call| match call.action.as_str() {
        "AddPortMapping" => Answer::Error(CONFLICT_IN_MAPPING_ENTRY),
        _ => Answer::Done,
    });
    let mapping = map(&search(&router), None).unwrap();
    assert_eq!(mapping.external_port, FIRST_DRAW ^ 1);
    let calls = router.calls();
    let actions: Vec<&str> = calls.iter().map(|call| call.action.as_str()).collect();
    assert_eq!(actions, ["AddPortMapping", "AddAnyPortMapping"]);
    assert_eq!(calls[1].arg("NewExternalPort"), FIRST_DRAW.to_string());
    assert_eq!(calls[1].arg("NewLeaseDuration"), "7200");
}

/// A router that maps a port only to the same port inside, 724 SamePortValuesRequired, gets the
/// port inside once its holder lets it go; one that refuses every port is asked for `DRAWS`
/// drawn ports, and then the map gives up.
#[test]
fn a_draw_falls_back_to_the_port_inside_or_gives_up_after_its_tries() {
    let adds = Arc::new(AtomicUsize::new(0));
    let router = {
        let adds = Arc::clone(&adds);
        FakeRouter::start(move |call| {
            let first = adds.fetch_add(1, Ordering::SeqCst) == 0;
            match call.arg("NewExternalPort") {
                "4433" if first => Answer::Error(CONFLICT_IN_MAPPING_ENTRY),
                "4433" => Answer::Done,
                _ => Answer::Error(SAME_PORT_VALUES_REQUIRED),
            }
        })
    };
    let mapping = map(&search(&router), None).unwrap();
    assert_eq!(mapping.external_port, 4433);
    assert_eq!(
        added_ports(&router),
        ["4433", &FIRST_DRAW.to_string(), "4433"]
    );

    let full = FakeRouter::start(|_| Answer::Error(CONFLICT_IN_MAPPING_ENTRY));
    let error = map(&search(&full), None).unwrap_err();
    assert!(
        matches!(
            error,
            MapError::UpnpAddAny(SoapError::Refused(CONFLICT_IN_MAPPING_ENTRY))
        ),
        "{error:?}"
    );
    // 4433, then each port drawn.
    assert_eq!(added_ports(&full).len(), 1 + DRAWS);
}

#[test]
fn a_stop_ends_a_map_between_two_requests() {
    // The router asks for a lease that never ends, 725, and the map stops as it answers.
    let stop = Arc::new(AtomicBool::new(false));
    let router = {
        let stop = Arc::clone(&stop);
        FakeRouter::start(move |_| {
            stop.store(true, Ordering::SeqCst);
            Answer::Error(725)
        })
    };
    let gateway = search(&router);
    let mut random = Random::seeded(0);
    let error = gateway.map(ask(None), &stop, &mut random).unwrap_err();
    assert!(matches!(error, MapError::Stopped), "{error:?}");
    let calls = router.calls();
    let actions: Vec<&str> = calls.iter().map(|call| call.action.as_str()).collect();
    assert_eq!(actions, ["AddPortMapping"]);

    // Stopped before it starts, it sends none.
    let error = gateway.map(ask(None), &stop, &mut random).unwrap_err();
    assert!(matches!(error, MapError::Stopped), "{error:?}");
    assert_eq!(router.calls().len(), 1);
}

#[test]
fn no_router_that_answers_the_search_is_an_error_at_its_timeout() {
    // A socket that reads the search and never answers.
    let silent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let error = UpnpGateway::search(
        silent.local_addr().unwrap(),
        Duration::from_millis(100),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(error, MapError::NoUpnpRouter), "{error:?}");
}
