use std::fs::File;
use std::io;
use std::path::Path;

use crate::os::Native;

/// A file opened to read only when its path's last name is no link: `O_NOFOLLOW` on Unix, as
/// OpenSSH opens a key, and the link itself with `FILE_FLAG_OPEN_REPARSE_POINT` on Windows, whose
/// handle then shows a link. The open waits on no pipe: `O_NONBLOCK` on Unix, which the reads of
/// a regular file ignore, so a pipe swapped in for a file opens at once and its handle shows it;
/// Windows names no pipe in the file system.
#[derive(Debug)]
pub struct NoFollow;

impl NoFollow {
    /// The file at `path`, open to read; none when its last name is a link, which is not
    /// followed.
    pub fn open(path: &Path) -> io::Result<Option<File>> {
        Native::open_no_follow(path)
    }
}
