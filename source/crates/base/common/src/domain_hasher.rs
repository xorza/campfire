use std::fmt;
use std::marker::PhantomData;

use crate::value32::Value32;

/// The domain tag of a BLAKE3 hash, `campfire/<name>/v<n>`, its context in BLAKE3's key
/// derivation mode, so no hash of one meaning equals one of another: `<name>` of lowercase
/// letters, digits and `-`, `<n>` of digits.
/// A tag is a constant of the code, `const DOMAIN: HashDomain<SessionId> = HashDomain::new(…)`,
/// so `new` refuses another spelling as it compiles; its type is the hash's, so a site's hash is
/// of the domain's type.
pub struct HashDomain<T> {
    tag: &'static str,
    output: PhantomData<fn() -> T>,
}

/// BLAKE3 of an input in key derivation mode, its domain's tag the context (05-protocol-spec.md,
/// Encoding and cryptography): the context is hashed apart from the input, so no input extends a
/// tag into another, as `campfire/a/v1` and `0` would `campfire/a/v10`. It is given its parts and
/// finished into the domain's type.
pub struct DomainHasher<T> {
    hasher: blake3::Hasher,
    output: PhantomData<fn() -> T>,
}

/// What a domain's hash is: a value of the hash's 32 bytes, a `Value32` of its meaning, or a
/// secret, which shows none of them.
pub trait HashOutput {
    fn of_hash(bytes: [u8; 32]) -> Self;
}

impl<T> HashDomain<T> {
    pub const fn new(tag: &'static str) -> Self {
        assert!(Self::spelled(tag.as_bytes()), "campfire/<name>/v<n>");
        Self {
            tag,
            output: PhantomData,
        }
    }

    pub const fn as_str(&self) -> &'static str {
        self.tag
    }

    /// Whether `tag` is `campfire/<name>/v<n>`.
    const fn spelled(tag: &[u8]) -> bool {
        const PREFIX: &[u8] = b"campfire/";
        if tag.len() <= PREFIX.len() {
            return false;
        }
        let mut at = 0;
        while at < PREFIX.len() {
            if tag[at] != PREFIX[at] {
                return false;
            }
            at += 1;
        }
        let name = at;
        while at < tag.len() && tag[at] != b'/' {
            let byte = tag[at];
            if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-') {
                return false;
            }
            at += 1;
        }
        if at == name || at + 2 >= tag.len() || tag[at + 1] != b'v' {
            return false;
        }
        at += 2;
        while at < tag.len() {
            if !tag[at].is_ascii_digit() {
                return false;
            }
            at += 1;
        }
        true
    }
}

impl<T> Clone for HashDomain<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for HashDomain<T> {}

impl<T> PartialEq for HashDomain<T> {
    fn eq(&self, other: &Self) -> bool {
        self.tag == other.tag
    }
}

impl<T> Eq for HashDomain<T> {}

impl<T> fmt::Debug for HashDomain<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("HashDomain").field(&self.tag).finish()
    }
}

impl<T: HashOutput> DomainHasher<T> {
    pub fn new(domain: HashDomain<T>) -> Self {
        Self {
            hasher: blake3::Hasher::new_derive_key(domain.as_str()),
            output: PhantomData,
        }
    }

    pub fn update(&mut self, bytes: &[u8]) -> &mut Self {
        self.hasher.update(bytes);
        self
    }

    /// The hash of the domain's tag and every part given, as the domain's type.
    pub fn finish(self) -> T {
        T::of_hash(*self.hasher.finalize().as_bytes())
    }
}

impl<T> Clone for DomainHasher<T> {
    fn clone(&self) -> Self {
        Self {
            hasher: self.hasher.clone(),
            output: PhantomData,
        }
    }
}

impl<T> fmt::Debug for DomainHasher<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("DomainHasher").field(&self.hasher).finish()
    }
}

impl<K> HashOutput for Value32<K> {
    fn of_hash(bytes: [u8; 32]) -> Self {
        Self::new(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::state_hash::StateHash;

    #[test]
    fn a_domain_hash_is_blake3_derive_key_of_its_tag_and_its_input() {
        let domain: HashDomain<StateHash> = HashDomain::new("campfire/test-hash/v1");
        let mut hasher = DomainHasher::new(domain);
        hasher.update(b"a").update(b"bc");
        assert_eq!(
            hasher.finish(),
            StateHash::new(blake3::derive_key("campfire/test-hash/v1", b"abc"))
        );
        // A version's digits and an input's do not run together.
        let ten: StateHash = DomainHasher::new(HashDomain::new("campfire/test-hash/v10")).finish();
        let mut one = DomainHasher::<StateHash>::new(HashDomain::new("campfire/test-hash/v1"));
        one.update(b"0");
        assert_ne!(one.finish(), ten);
        let other: StateHash = DomainHasher::new(HashDomain::new("campfire/test-hash/v2")).finish();
        assert_ne!(other, DomainHasher::new(domain).finish());
        // A tag is `campfire/`, a name, and `/v` with a version.
        let spelled = |tag: &str| HashDomain::<StateHash>::spelled(tag.as_bytes());
        for tag in ["campfire/session-id/v3", "campfire/a/v10"] {
            assert!(spelled(tag), "{tag}");
        }
        for tag in [
            "",
            "campfire/",
            "campfire//v1",
            "campfire/Name/v1",
            "campfire/name/v",
            "campfire/name/1",
            "campfire/name/v1x",
            "other/name/v1",
        ] {
            assert!(!spelled(tag), "{tag}");
        }
    }
}
