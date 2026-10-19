use crate::registry_hasher::RegistryHash;

/// Whether a protocol registry takes registrations, or is sealed with the hash its peers compare: a
/// registration changes the protocol and the systems built from it, so none comes after the seal
/// (13-net-stack/api.md, Registration is order-independent and ends at `finish`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RegistrySeal {
    #[default]
    Open,
    Sealed(RegistryHash),
}

impl RegistrySeal {
    /// # Panics
    ///
    /// When sealed: `registry` names the registry in the message.
    pub fn assert_open(self, registry: &str) {
        assert!(
            matches!(self, Self::Open),
            "the {registry} takes a registration after its plugin finished, which sealed it"
        );
    }

    /// Seals the registry with `hash`.
    ///
    /// # Panics
    ///
    /// When sealed already: one plugin seals a registry once.
    pub fn seal(&mut self, registry: &str, hash: RegistryHash) {
        self.assert_open(registry);
        *self = Self::Sealed(hash);
    }

    /// The hash the registry was sealed with, or `None` while it is open.
    pub const fn hash(self) -> Option<RegistryHash> {
        match self {
            Self::Open => None,
            Self::Sealed(hash) => Some(hash),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seal_holds_its_hash() {
        let mut seal = RegistrySeal::default();
        assert_eq!(seal.hash(), None);
        seal.assert_open("test registry");
        let hash = RegistryHash::new([7; 32]);
        seal.seal("test registry", hash);
        assert_eq!(seal.hash(), Some(hash));
    }

    #[test]
    #[should_panic(expected = "the test registry takes a registration after its plugin finished")]
    fn a_sealed_registry_refuses_a_registration() {
        let mut seal = RegistrySeal::default();
        seal.seal("test registry", RegistryHash::new([7; 32]));
        seal.assert_open("test registry");
    }
}
