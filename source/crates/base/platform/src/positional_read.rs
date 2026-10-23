use std::fs::File;
use std::io;

use crate::os::Native;

/// A read of a file's bytes at an offset that moves no cursor of the handle, so readers through
/// one shared handle never race: `pread` on Unix, `ReadFile` with an offset on Windows.
#[derive(Debug)]
pub struct PositionalRead;

impl PositionalRead {
    /// Fills `buf` from `offset` of `file`; `UnexpectedEof` when the file ends first.
    pub fn exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
        Native::read_exact_at(file, buf, offset)
    }
}
