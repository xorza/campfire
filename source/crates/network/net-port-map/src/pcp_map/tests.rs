use super::*;

const NONCE: MapNonce = MapNonce([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);

/// A granting reply as RFC 6887 §7.2 and §11.1 lay it out, by hand: version 2, the reply bit and
/// opcode 1, result 0, a lifetime of 3,600 s (0x0e10), an epoch of 1,000 s (0x03e8), 12 reserved
/// bytes, the nonce, UDP, 3 reserved bytes, internal port 4,433 (0x1151), external port 50,000
/// (0xc350), and 203.0.113.7 as IPv4-mapped IPv6.
fn granted() -> Vec<u8> {
    let mut bytes = vec![2, 0x81, 0, 0, 0, 0, 0x0e, 0x10, 0, 0, 0x03, 0xe8];
    bytes.extend([0; 12]);
    bytes.extend(NONCE.0);
    bytes.extend([17, 0, 0, 0, 0x11, 0x51, 0xc3, 0x50]);
    bytes.extend([0; 10]);
    bytes.extend([0xff, 0xff, 203, 0, 113, 7]);
    bytes
}

#[test]
fn a_map_request_lays_out_its_fields_as_rfc_6887_does() {
    let request = PcpMap {
        nonce: NONCE,
        client: Ipv4Addr::new(192, 168, 0, 5),
        internal_port: 4433,
        external: SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0),
        lifetime: Duration::from_secs(7200),
    };
    // Version 2, opcode 1 with the reply bit clear, 2 reserved bytes, 7,200 s (0x1c20), the
    // client as ::ffff:192.168.0.5; the nonce, UDP, 3 reserved bytes, 4,433 (0x1151), no
    // suggested port, and the IPv4 all-zeros address ::ffff:0.0.0.0.
    let mut expected = vec![2, 1, 0, 0, 0, 0, 0x1c, 0x20];
    expected.extend([0; 10]);
    expected.extend([0xff, 0xff, 192, 168, 0, 5]);
    expected.extend(NONCE.0);
    expected.extend([17, 0, 0, 0, 0x11, 0x51, 0, 0]);
    expected.extend([0; 10]);
    expected.extend([0xff, 0xff, 0, 0, 0, 0]);
    assert_eq!(request.encode().as_slice(), expected.as_slice());

    // A renewal suggests the assigned address; a deletion asks for a lifetime of 0.
    let renewal = PcpMap {
        external: SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 50_000),
        lifetime: Duration::ZERO,
        ..request
    };
    let bytes = renewal.encode();
    assert_eq!(bytes[4..8], [0; 4]);
    assert_eq!(bytes[42..44], [0xc3, 0x50]);
    assert_eq!(bytes[54..60], [0xff, 0xff, 203, 0, 113, 7]);
}

#[test]
fn a_reply_grants_refuses_or_names_a_nat_pmp_gateway() {
    assert_eq!(
        PcpReply::decode(&granted()),
        Ok(PcpReply::Granted(PcpMapping {
            nonce: NONCE,
            internal_port: 4433,
            external: SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 50_000),
            lifetime: Duration::from_secs(3600),
        }))
    );
    // A refusal is the header alone: result 8, NO_RESOURCES, holding for 30 s.
    let mut refused = vec![2, 0x81, 0, 8, 0, 0, 0, 30];
    refused.extend([0; 16]);
    assert_eq!(
        PcpReply::decode(&refused),
        Ok(PcpReply::Refused {
            refusal: PcpRefusal::NoResources,
            lasts: Duration::from_secs(30),
        })
    );
    // NAT-PMP's answer to an unknown version: version 0, opcode 128, result 1, an epoch.
    assert_eq!(
        PcpReply::decode(&[0, 128, 0, 1, 0, 0, 0, 5]),
        Ok(PcpReply::NatPmpOnly)
    );
}

#[test]
fn a_reply_that_does_not_read_is_refused_by_its_flaw() {
    let edit = |at: usize, value: u8| {
        let mut bytes = granted();
        bytes[at] = value;
        bytes
    };
    let mut ipv6 = granted();
    ipv6[44..60].copy_from_slice(&Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1).octets());
    for (bytes, error) in [
        (vec![], ReplyError::TooShort { len: 0, needed: 24 }),
        (vec![3], ReplyError::Version(3)),
        (
            granted()[..23].to_vec(),
            ReplyError::TooShort {
                len: 23,
                needed: 24,
            },
        ),
        (
            granted()[..59].to_vec(),
            ReplyError::TooShort {
                len: 59,
                needed: 60,
            },
        ),
        // A request's opcode octet, the reply bit clear.
        (edit(1, 1), ReplyError::Opcode(1)),
        (edit(3, 14), ReplyError::Result(14)),
        (edit(36, 6), ReplyError::Protocol(6)),
        (ipv6, ReplyError::NotIpv4),
    ] {
        assert_eq!(PcpReply::decode(&bytes), Err(error), "{bytes:?}");
    }
}
