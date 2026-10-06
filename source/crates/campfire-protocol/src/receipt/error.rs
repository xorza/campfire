use thiserror::Error;

/// Why a file's bytes are no receipt file.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReceiptFileError {
    /// The bytes do not start with the file's tag.
    #[error("not a receipt file")]
    NotReceipt,
    #[error("does not decode: {0}")]
    Malformed(#[source] postcard::Error),
    /// Bytes remain after the receipt.
    #[error("bytes after the receipt")]
    Trailing,
}
