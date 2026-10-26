use super::*;

fn hash(byte: u8) -> RegistryHash {
    RegistryHash::new([byte; 32])
}

/// The hashes encode by their registries' names, whatever order the layers sealed them in: two
/// apps of one build write the same bytes. One hash that differs, a registry one side lacks and
/// bytes that read as no check each name what differs.
#[test]
fn a_check_encodes_by_name_and_names_what_differs() {
    let mut local = ProtocolHashes::default();
    local.add("message registry", hash(2));
    local.add("channel registry", hash(1));
    let mut sealed_apart = ProtocolHashes::default();
    sealed_apart.add("channel registry", hash(1));
    sealed_apart.add("message registry", hash(2));
    let bytes = local.bytes();
    assert_eq!(bytes, sealed_apart.bytes());
    // 16 bytes of name and its length byte, then 32 of hash: "channel registry" first.
    assert_eq!(bytes.len(), 2 * (1 + 16 + 32));
    assert_eq!(bytes[0], 16);
    assert_eq!(&bytes[1..17], b"channel registry");
    assert!(local.differing(&bytes).is_empty());

    let mut other = ProtocolHashes::default();
    other.add("channel registry", hash(1));
    other.add("message registry", hash(3));
    other.add("component registry", hash(4));
    assert_eq!(
        local.differing(&other.bytes()),
        ["message registry", "component registry"]
    );
    assert_eq!(
        local.differing(&bytes[..10]),
        ["10 bytes that read as no check"]
    );
}
