use std::fmt;

use campfire_codec::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The first 8 bytes of the BLAKE3 hash of a JSON schema, as of a console's requests and lines,
/// which `Output::fingerprint` gives, so two builds whose types differ in any field, name or case
/// tell each other apart before either reads the other wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Fingerprint(u64);

impl Fingerprint {
    /// The fingerprint of the schema of `T`.
    pub(crate) fn of<T: JsonSchema>() -> Self {
        let schema = Json::write(&schemars::schema_for!(T));
        let hash = blake3::hash(schema.as_bytes());
        let (head, _) = hash
            .as_bytes()
            .split_first_chunk()
            .expect("a hash of 32 bytes");
        Self(u64::from_le_bytes(*head))
    }
}

/// As 16 hexadecimal digits.
impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

#[cfg(test)]
mod tests;
