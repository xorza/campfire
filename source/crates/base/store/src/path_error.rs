use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// A failure of a call on the file or directory at `path`: its case, `error`, and the path, so a
/// caller logs it as it is and wraps no path of its own, as the `fs-err` crate adds paths to
/// std's errors. Its text is the path, and its source the case, so a report reads
/// `path: case: cause`.
#[derive(Debug)]
pub struct PathError<E> {
    pub path: PathBuf,
    pub error: E,
}

impl<E> PathError<E> {
    /// Gives `error` the path `path`, as `map_err` takes it; the path is copied only on a
    /// failure, so a call that succeeds allocates nothing for it.
    pub(crate) fn at(path: &Path) -> impl FnOnce(E) -> PathError<E> + '_ {
        move |error| PathError {
            path: path.to_owned(),
            error,
        }
    }
}

impl<E> fmt::Display for PathError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.path.display().fmt(f)
    }
}

impl<E: Error + 'static> Error for PathError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}
