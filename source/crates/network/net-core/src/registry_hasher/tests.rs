use super::*;

const DOMAIN: HashDomain<RegistryHash> = HashDomain::new("campfire/test-registry/v1");

fn hash_of(build: impl FnOnce(&mut RegistryHasher)) -> RegistryHash {
    let mut hasher = RegistryHasher::new(DOMAIN);
    build(&mut hasher);
    hasher.finish()
}

/// The same entries in the same order agree, and are the domain hash of each name followed
/// by 0xff; another order, another name, other bytes, names split elsewhere or another domain
/// differ.
#[test]
fn entries_order_and_bytes_each_change_the_hash() {
    let base = hash_of(|h| {
        h.name("u8");
        h.name("u16");
    });
    let mut expected = DomainHasher::new(DOMAIN);
    expected.update(b"u8\xffu16\xff");
    assert_eq!(base, expected.finish());
    let reordered = hash_of(|h| {
        h.name("u16");
        h.name("u8");
    });
    let other_name = hash_of(|h| {
        h.name("u8");
        h.name("u32");
    });
    let with_bytes = hash_of(|h| {
        h.name("u8");
        h.write(&[1]);
        h.name("u16");
    });
    let split = hash_of(|h| {
        h.name("u8u");
        h.name("16");
    });
    let mut other_domain = RegistryHasher::new(HashDomain::new("campfire/other-test-registry/v1"));
    other_domain.name("u8");
    other_domain.name("u16");
    for other in [
        reordered,
        other_name,
        with_bytes,
        split,
        other_domain.finish(),
    ] {
        assert_ne!(base, other);
    }
}
