use std::str;

use campfire_codec::{Binary, Toml};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::ClientFileError;

/// The format a client file is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientFormat {
    Toml,
    Binary,
}

/// A client file of a package, which reads and writes in the one format it declares, so the
/// import that writes it and the client that reads it take one form of it.
pub trait ClientFile: Serialize + DeserializeOwned {
    const FORMAT: ClientFormat;

    /// The file `bytes` hold; an error for bytes its format or its rules refuse.
    fn decode(bytes: &[u8]) -> Result<Self, ClientFileError> {
        Ok(match Self::FORMAT {
            ClientFormat::Toml => Toml::parse(str::from_utf8(bytes)?)?,
            ClientFormat::Binary => Binary::decode(bytes)?,
        })
    }

    /// The file's bytes.
    fn encode(&self) -> Vec<u8> {
        match Self::FORMAT {
            ClientFormat::Toml => Toml::write(self).into_bytes(),
            ClientFormat::Binary => Binary::encode(self),
        }
    }
}
