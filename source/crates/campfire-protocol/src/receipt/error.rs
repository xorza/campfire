use campfire_common::BinaryError;
use thiserror::Error;

/// Why a file's bytes are no receipt file.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReceiptFileError {
    /// The bytes do not start with the file's tag.
    #[error("not a receipt file")]
    NotReceipt,
    #[error("does not decode")]
    Malformed(#[source] BinaryError),
    /// Bytes remain after the receipt.
    #[error("bytes after the receipt")]
    Trailing,
}
