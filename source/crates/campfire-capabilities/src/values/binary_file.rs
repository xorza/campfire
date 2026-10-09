use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::values::error::DecodeError;

/// A binary file of a package, postcard with no schema, which the package API covers by its
/// layout: it encodes one way, and decodes only from that encoding, as postcard's decoder takes
/// an over-long integer and ignores trailing bytes.
pub trait BinaryFile: Serialize + DeserializeOwned {
    fn encode(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("a package's binary file encodes")
    }

    /// The value `bytes` hold; an error for bytes that hold none its type's checks take, or
    /// that are not its encoding.
    fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let value: Self = postcard::from_bytes(bytes).map_err(DecodeError::Malformed)?;
        if value.encode() != bytes {
            return Err(DecodeError::NotCanonical);
        }
        Ok(value)
    }
}
