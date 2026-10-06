use std::error::Error;
use std::fmt;

/// Why a file's bytes are no receipt file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptFileError {
    /// The bytes do not start with the file's tag.
    NotReceipt,
    Malformed(postcard::Error),
    /// Bytes remain after the receipt.
    Trailing,
}

impl fmt::Display for ReceiptFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReceiptFileError::NotReceipt => f.write_str("not a receipt file"),
            ReceiptFileError::Malformed(error) => write!(f, "does not decode: {error}"),
            ReceiptFileError::Trailing => f.write_str("bytes after the receipt"),
        }
    }
}

impl Error for ReceiptFileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ReceiptFileError::Malformed(error) => Some(error),
            ReceiptFileError::NotReceipt | ReceiptFileError::Trailing => None,
        }
    }
}
