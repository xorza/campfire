use std::path::Path;

use campfire_package_format::ContentError;
use campfire_store::{PathError, ReadError};
use campfire_values::PackagePath;

/// How many bytes a read of a package's file takes: exactly a row's size, or no more than a
/// bound, as the index's own read takes. No read takes a file whole, however long it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadBound {
    Exact(u64),
    AtMost(u64),
}

impl ReadBound {
    /// The length to read of a file of `len` bytes at `path`, `file` where its place holds it:
    /// `Changed` for a length other than an exact bound's, and too large for one past an upper
    /// bound.
    pub(crate) fn take(
        self,
        len: u64,
        path: &PackagePath,
        file: &Path,
    ) -> Result<usize, ContentError> {
        match self {
            Self::Exact(size) if len != size => Err(ContentError::Changed { path: path.clone() }),
            Self::AtMost(max) if len > max => {
                let max = usize::try_from(max).expect("a bound fits usize");
                let error = PathError {
                    path: file.to_owned(),
                    error: ReadError::TooLarge { max },
                };
                Err(ContentError::Read(Box::new(error)))
            }
            _ => Ok(usize::try_from(len).expect("a bounded length fits usize")),
        }
    }
}

#[cfg(test)]
mod tests;
