use core::hash::Hasher;

use crate::stable_hasher::StableHasher;

/// Hashes a registry's entries into the value peers compare before data flows: the registry's
/// seal feeds each entry as it stands then, in the order of its id, so the hash holds the
/// protocol and not the order of the calls that built it.
///
/// A type enters by its `core::any::type_name`, which the compiler does not promise to keep
/// across compiler versions: peers built from the same source by the same compiler agree, and
/// the protocol check verifies that they do. Values enter as explicit little-endian bytes, so a
/// 32-bit peer and a 64-bit peer hash them alike.
#[derive(Debug, Clone, Default)]
pub struct RegistryHasher {
    hasher: StableHasher,
}

pub type RegistryHash = u64;

impl RegistryHasher {
    /// Adds an entry's type, by its name.
    pub fn name(&mut self, name: &str) {
        self.hasher.write(name.as_bytes());
        // A terminator keeps `("ab", "c")` and `("a", "bc")` apart.
        self.hasher.write(&[0xff]);
    }

    /// Adds bytes that describe the entry named last, such as its settings.
    pub fn write(&mut self, bytes: &[u8]) {
        self.hasher.write(bytes);
    }

    pub fn finish(&self) -> RegistryHash {
        self.hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_of(build: impl FnOnce(&mut RegistryHasher)) -> RegistryHash {
        let mut hasher = RegistryHasher::default();
        build(&mut hasher);
        hasher.finish()
    }

    /// The same entries in the same order agree; another order, another name, other bytes or
    /// names split elsewhere differ.
    #[test]
    fn entries_order_and_bytes_each_change_the_hash() {
        let base = hash_of(|h| {
            h.name("u8");
            h.name("u16");
        });
        assert_eq!(
            base,
            hash_of(|h| {
                h.name("u8");
                h.name("u16");
            })
        );
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
        for other in [reordered, other_name, with_bytes, split] {
            assert_ne!(base, other);
        }
    }
}
