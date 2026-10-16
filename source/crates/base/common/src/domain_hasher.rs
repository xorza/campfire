/// The domain tag that starts a BLAKE3 hash's input, `campfire/<name>/v<n>`, so no hash of one
/// meaning equals one of another: `<name>` of lowercase letters, digits and `-`, `<n>` of digits.
/// A tag is a constant of the code, `const DOMAIN: HashDomain = HashDomain::new(…)`, so `new`
/// refuses another spelling as it compiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashDomain(&'static str);

/// BLAKE3 of an input led by its domain's tag (design 05, Encoding and cryptography).
#[derive(Debug, Clone)]
pub struct DomainHasher(blake3::Hasher);

impl HashDomain {
    pub const fn new(tag: &'static str) -> Self {
        assert!(Self::spelled(tag.as_bytes()), "campfire/<name>/v<n>");
        Self(tag)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
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

impl DomainHasher {
    pub fn new(domain: HashDomain) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(domain.as_str().as_bytes());
        Self(hasher)
    }

    pub fn update(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.update(bytes);
        self
    }

    /// The hash of the domain's tag and every part given.
    pub fn finish(&self) -> [u8; 32] {
        *self.0.finalize().as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_domain_hash_is_blake3_of_its_tag_then_its_input() {
        let domain = HashDomain::new("campfire/test-hash/v1");
        let mut hasher = DomainHasher::new(domain);
        hasher.update(b"a").update(b"bc");
        assert_eq!(
            hasher.finish(),
            *blake3::hash(b"campfire/test-hash/v1abc").as_bytes()
        );
        let other = DomainHasher::new(HashDomain::new("campfire/test-hash/v2")).finish();
        assert_ne!(other, DomainHasher::new(domain).finish());
        // A tag is `campfire/`, a name, and `/v` with a version.
        let spelled = |tag: &str| HashDomain::spelled(tag.as_bytes());
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
