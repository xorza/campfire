use std::fs::File;
use std::io::{self, Write};

/// What a journal's writer thread appends to and syncs: its file, or a stand-in under test.
pub trait JournalFile: Send + 'static {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;

    /// Makes what was appended durable.
    fn sync(&mut self) -> io::Result<()>;
}

/// `sync_data`, which is `F_FULLFSYNC` on Apple systems: a filesystem without it fails the sync.
impl JournalFile for File {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.write_all(bytes)
    }

    fn sync(&mut self) -> io::Result<()> {
        self.sync_data()
    }
}
