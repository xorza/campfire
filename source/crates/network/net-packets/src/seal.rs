use campfire_net_payload::PacketNumber;

use crate::open_error::OpenError;

/// The bytes a seal appends to each payload: an AEAD tag.
pub const TAG_LEN: usize = 16;

/// The AEAD of a sealed session's two directions, which its IO's establishment gives: the session
/// does not depend on the crypto that implements it.
pub trait Seal: Send + Sync + core::fmt::Debug {
    /// Encrypts `payload` in place under `number`, with `header` as associated data, and returns
    /// the tag.
    fn seal(&self, number: PacketNumber, header: &[u8], payload: &mut [u8]) -> [u8; TAG_LEN];

    /// Decrypts `sealed`, its tag last, in place under the peer's `number`, with `header` as
    /// associated data, and returns the plaintext's length.
    fn open(
        &self,
        number: PacketNumber,
        header: &[u8],
        sealed: &mut [u8],
    ) -> Result<usize, OpenError>;
}
