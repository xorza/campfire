use std::fs::File;
use std::io;
use std::path::Path;

use crate::platform::os::Native;

/// A file opened to read only when its path's last name is no link: `O_NOFOLLOW` on Unix, as
/// OpenSSH opens a key, and the link itself with `FILE_FLAG_OPEN_REPARSE_POINT` on Windows, whose
/// handle then shows a link.
#[derive(Debug)]
pub(crate) struct NoFollow;

impl NoFollow {
    /// The file at `path`, open to read; none when its last name is a link, which is not
    /// followed.
    pub(crate) fn open(path: &Path) -> io::Result<Option<File>> {
        Native::open_no_follow(path)
    }
}
