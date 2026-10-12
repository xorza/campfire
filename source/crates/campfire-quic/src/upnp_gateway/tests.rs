use super::*;
use crate::upnp_gateway::fake_router::{Answer, Call, FakeRouter};

const LIFETIME: Duration = Duration::from_secs(7200);
const EXTERNAL_IP: Ipv4Addr = FakeRouter::EXTERNAL_IP;

fn search(router: &FakeRouter) -> UpnpGateway {
    UpnpGateway::search(router.ssdp(), Duration::from_millis(500)).unwrap()
}

#[test]
fn a_router_maps_the_port_at_the_same_port_renews_it_and_deletes_it() {
    let router = FakeRouter::start(|_| Answer::Done);
    let gateway = search(&router);
    let mapping = gateway.map(4433, None, LIFETIME).unwrap();
    assert_eq!(
        mapping,
        Mapping {
            external: SocketAddrV4::new(EXTERNAL_IP, 4433),
            lifetime: LIFETIME,
            by: MappedBy::Upnp,
        }
    );
    assert_eq!(
        gateway.map(4433, Some(&mapping), LIFETIME).unwrap(),
        mapping
    );
    gateway.delete(&mapping).unwrap();
    let calls = router.calls();
    let actions: Vec<&str> = calls.iter().map(|call| call.action.as_str()).collect();
    assert_eq!(
        actions,
        [
            "AddPortMapping",
            "GetExternalIPAddress",
            "AddPortMapping",
            "GetExternalIPAddress",
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
        ("NewPortMappingDescription", "campfire"),
    ] {
        assert_eq!(add.arg(arg), value, "{arg}");
    }
    assert_eq!(calls[4].arg("NewExternalPort"), "4433");
    assert_eq!(calls[4].arg("NewProtocol"), "UDP");
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
        let mapping = search(&router).map(4433, None, LIFETIME).unwrap();
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
            assert_eq!(adds[2].0, mapping.external.port().to_string());
            assert_ne!(mapping.external.port(), 4433);
        } else {
            assert_eq!(leases, ["7200", "0"]);
            assert_eq!(mapping.external.port(), 4433);
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
    let mapping = gateway.map(4433, None, LIFETIME).unwrap();
    let adds: Vec<Call> = router
        .calls()
        .into_iter()
        .filter(|call| call.action == "AddPortMapping")
        .collect();
    // The router lacks AddAnyPortMapping, so `igd-next` asks for a port it draws.
    assert_eq!(adds.len(), 2);
    let drawn: u16 = adds[1].arg("NewExternalPort").parse().unwrap();
    assert_ne!(drawn, 4433);
    assert_eq!(adds[1].arg("NewInternalPort"), "4433");
    assert_eq!(mapping.external, SocketAddrV4::new(EXTERNAL_IP, drawn));

    let held = Mapping {
        external: SocketAddrV4::new(EXTERNAL_IP, 4433),
        ..mapping
    };
    let error = gateway.map(4433, Some(&held), LIFETIME).unwrap_err();
    assert!(
        matches!(error, MapError::UpnpAdd(AddPortError::PortInUse)),
        "{error:?}"
    );
}

#[test]
fn no_router_that_answers_the_search_is_an_error_at_its_timeout() {
    // A socket that reads the search and never answers.
    let silent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let error =
        UpnpGateway::search(silent.local_addr().unwrap(), Duration::from_millis(100)).unwrap_err();
    assert!(matches!(error, MapError::UpnpSearch(_)), "{error:?}");
}
