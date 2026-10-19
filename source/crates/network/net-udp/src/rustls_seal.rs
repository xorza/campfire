use campfire_net_packets::{OpenError, Seal, TAG_LEN};
use campfire_net_payload::PacketNumber;
use rustls::quic::{Keys, PacketKey};

/// A sealed packet session's AEAD: the 1-RTT keys of its TLS handshake, ChaCha20-Poly1305 with each
/// packet's nonce from its number (RFC 9001 §5.3).
pub(crate) struct RustlsSeal {
    local: Box<dyn PacketKey>,
    remote: Box<dyn PacketKey>,
}

impl RustlsSeal {
    pub(crate) fn new(keys: Keys) -> Self {
        Self {
            local: keys.local.packet,
            remote: keys.remote.packet,
        }
    }
}

impl core::fmt::Debug for RustlsSeal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("RustlsSeal(..)")
    }
}

impl Seal for RustlsSeal {
    fn seal(&self, number: PacketNumber, header: &[u8], payload: &mut [u8]) -> [u8; TAG_LEN] {
        let tag = self
            .local
            .encrypt_in_place(number.0, header, payload)
            .expect("a packet session sends fewer packets than ChaCha20-Poly1305's limit");
        let mut bytes = [0; TAG_LEN];
        bytes.copy_from_slice(tag.as_ref());
        bytes
    }

    fn open(
        &self,
        number: PacketNumber,
        header: &[u8],
        sealed: &mut [u8],
    ) -> Result<usize, OpenError> {
        self.remote
            .decrypt_in_place(number.0, header, sealed)
            .map(<[u8]>::len)
            .map_err(|_| OpenError)
    }
}
