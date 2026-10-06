use std::str::Utf8Error;

use nostr::error::Error as NostrError;
use thiserror::Error;

/// Why bytes hold no `nsec`.
#[derive(Debug, Error)]
pub enum NsecError {
    #[error("the key is no text: {0}")]
    NotText(#[source] Utf8Error),
    #[error("the key is no nsec: {0}")]
    NotNsec(#[source] NostrError),
}
