use campfire_common::{DomainHasher, HashDomain, Meaning, Value32};

/// Hashes a registry's entries into the value peers compare before data flows: the registry's seal
/// feeds each entry as it stands then, in the order of its id, so the hash holds the protocol and
/// not the order of the calls that built it (11-net-stack/api.md, The registry hash is a domain
/// hash) (11-net-stack/api.md, A registry's hash holds its final entries, in the order of their
/// ids).
///
/// A type enters by its `core::any::type_name`, which the compiler does not promise to keep
/// across compiler versions: peers built from the same source by the same compiler agree, and
/// the protocol check verifies that they do. Values enter as explicit little-endian bytes, so a
/// 32-bit peer and a 64-bit peer hash them alike.
#[derive(Debug, Clone)]
pub struct RegistryHasher(DomainHasher<RegistryHash>);

/// The hash of a sealed protocol registry.
pub type RegistryHash = Value32<RegistryHashMeaning>;

/// The meaning of a `RegistryHash`.
#[derive(Debug)]
pub enum RegistryHashMeaning {}

impl Meaning for RegistryHashMeaning {
    const NAME: &'static str = "RegistryHash";
}

impl RegistryHasher {
    /// A hasher of the registry whose hashes `domain` tags.
    pub fn new(domain: HashDomain<RegistryHash>) -> Self {
        Self(DomainHasher::new(domain))
    }

    /// Adds an entry's type, by its name.
    pub fn name(&mut self, name: &str) {
        // A byte no UTF-8 name holds keeps `("ab", "c")` and `("a", "bc")` apart.
        self.0.update(name.as_bytes()).update(&[0xff]);
    }

    /// Adds bytes that describe the entry named last, such as its settings.
    pub fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub fn finish(self) -> RegistryHash {
        self.0.finish()
    }
}

#[cfg(test)]
mod tests {
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
        let mut other_domain =
            RegistryHasher::new(HashDomain::new("campfire/other-test-registry/v1"));
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
}
