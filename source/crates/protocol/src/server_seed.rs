use blake3::Hasher;

/// Starts the commitment, so no other BLAKE3 use can produce one.
const COMMITMENT_DOMAIN: &[u8] = b"campfire/seed-commitment/v1";

/// The server's secret for a segment's randomness. Until the segment is published, the header
/// holds only its commitment: the seed predicts every hidden random outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSeed([u8; 32]);

/// `BLAKE3(domain ‖ server seed)`: it binds the server to its seed before the players add theirs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SeedCommitment([u8; 32]);

impl ServerSeed {
    pub const fn new(bytes: [u8; 32]) -> ServerSeed {
        ServerSeed(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn commitment(&self) -> SeedCommitment {
        let mut hasher = Hasher::new();
        hasher.update(COMMITMENT_DOMAIN).update(&self.0);
        SeedCommitment(*hasher.finalize().as_bytes())
    }
}

impl SeedCommitment {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
