use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::error::Base64Error;

/// Base64, as RFC 4648 writes it with padding: the text an MCP result carries an image's bytes
/// in.
#[derive(Debug)]
pub struct Base64;

impl Base64 {
    /// The text of `bytes`.
    pub fn encode(bytes: &[u8]) -> String {
        STANDARD.encode(bytes)
    }

    /// The bytes `text` holds; an error when it is no base64.
    pub fn decode(text: &str) -> Result<Vec<u8>, Base64Error> {
        Ok(STANDARD.decode(text)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_encode_as_rfc_4648_writes_them_and_decode_back() {
        // RFC 4648's own vectors: "foob" pads with two, "fooba" with one.
        assert_eq!(Base64::encode(b"foob"), "Zm9vYg==");
        assert_eq!(Base64::encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(Base64::encode(b""), "");
        assert_eq!(Base64::decode("Zm9vYmE=").unwrap(), b"fooba");
        assert!(Base64::decode("Zm9v!").is_err());
    }
}
