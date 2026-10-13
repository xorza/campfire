use std::fs::File;
use std::io::{self, Write};

/// What an append writer's thread appends to and syncs: a file, or a stand-in under test.
pub trait AppendFile: Send + 'static {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;

    /// Makes what was appended durable.
    fn sync(&mut self) -> io::Result<()>;
}

/// `sync_data`, which is `F_FULLFSYNC` on Apple systems: a filesystem without it fails the sync.
impl AppendFile for File {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.write_all(bytes)
    }

    fn sync(&mut self) -> io::Result<()> {
        self.sync_data()
    }
}
