use bevy_ecs::resource::Resource;
use bytes::{BufMut, Bytes, BytesMut};
use campfire_net_core::RegistryHash;

/// The hash of each of the app's sealed registries, by the registry's name, as each layer's plugin
/// seals its registry: what a peer's protocol check compares with its own, byte for byte, so a
/// peer of another build unlinks before data flows (11-net-stack/transport.md, Both peers agree on
/// the protocol before data flows).
#[derive(Resource, Debug, Default)]
pub struct ProtocolHashes {
    /// Each registry's hash, by its name.
    hashes: Vec<(&'static str, RegistryHash)>,
    /// The check's bytes, written again as each registry adds its hash, so each link's check
    /// shares them.
    encoded: Bytes,
}

impl ProtocolHashes {
    /// Adds the hash of the registry `registry`, as its plugin seals it; each registry once.
    pub fn add(&mut self, registry: &'static str, hash: RegistryHash) {
        debug_assert!(
            self.hashes.iter().all(|&(held, _)| held != registry),
            "the {registry} is sealed once"
        );
        let at = self.hashes.partition_point(|&(held, _)| held < registry);
        self.hashes.insert(at, (registry, hash));
        self.encoded = self.encode();
    }

    /// The check's bytes, as `encode` writes them.
    pub(crate) fn bytes(&self) -> Bytes {
        self.encoded.clone()
    }

    /// The check's bytes: each registry's name, as one byte of length and its text, then its
    /// hash, in the order of their names.
    fn encode(&self) -> Bytes {
        let mut bytes = BytesMut::new();
        for &(registry, hash) in &self.hashes {
            let len = u8::try_from(registry.len()).expect("a registry's name is short");
            bytes.put_u8(len);
            bytes.put_slice(registry.as_bytes());
            bytes.put_slice(&hash.get());
        }
        bytes.freeze()
    }

    /// The registries whose hashes the check `remote` gives differently or lacks, and those it
    /// gives that this end has none of, by name, as a mismatch logs them; the whole check when it
    /// does not read as one.
    pub(crate) fn differing(&self, remote: &[u8]) -> Vec<String> {
        let Some(remote) = Self::decode(remote) else {
            return vec![format!("{} bytes that read as no check", remote.len())];
        };
        let mut differing: Vec<String> = (self.hashes.iter())
            .filter(|&&(registry, hash)| {
                !remote
                    .iter()
                    .any(|(held, theirs)| held == registry && *theirs == hash)
            })
            .map(|&(registry, _)| registry.to_owned())
            .collect();
        differing.extend(
            (remote.into_iter())
                .filter(|(held, _)| self.hashes.iter().all(|&(registry, _)| registry != held))
                .map(|(held, _)| held),
        );
        differing
    }

    /// The registries and hashes of the check `bytes`, as `encode` writes them; none for bytes
    /// that do not read so.
    fn decode(mut bytes: &[u8]) -> Option<Vec<(String, RegistryHash)>> {
        let mut entries = Vec::new();
        while let Some((&len, rest)) = bytes.split_first() {
            let (name, rest) = rest.split_at_checked(usize::from(len))?;
            let (hash, rest) = rest.split_first_chunk::<32>()?;
            let name = str::from_utf8(name).ok()?.to_owned();
            entries.push((name, RegistryHash::new(*hash)));
            bytes = rest;
        }
        Some(entries)
    }
}

#[cfg(test)]
mod tests;
