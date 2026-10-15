use std::collections::BTreeMap;
use std::io::{self, Read};

use campfire_common::{ContentHash, ContentHasher};
use campfire_values::PackagePath;

use crate::error::ContentError;
use crate::file_index::{FileIndex, FileRow};

/// One walk over a package's files, as a package is packed: each file's size and SHA-256, hashed
/// as it streams past and not kept, into the package's index.
#[derive(Debug, Default)]
pub struct PackageWalk {
    rows: BTreeMap<PackagePath, FileRow>,
    /// The buffer a file passes through, made at its first use.
    stream: Vec<u8>,
}

/// The size of the buffer a file passes through.
const STREAM_CHUNK: usize = 64 * 1024;

impl PackageWalk {
    pub const fn new() -> PackageWalk {
        PackageWalk {
            rows: BTreeMap::new(),
            stream: Vec::new(),
        }
    }

    /// Takes the file at `path` whose bytes `file` gives, to its end; their SHA-256.
    pub fn add(
        &mut self,
        path: PackagePath,
        mut file: impl Read,
    ) -> Result<ContentHash, ContentError> {
        self.stream.resize(STREAM_CHUNK, 0);
        let mut hasher = ContentHasher::default();
        let mut size = 0_u64;
        loop {
            let count = match file.read(&mut self.stream) {
                Ok(0) => break,
                Ok(count) => count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(ContentError::Io { path, error }),
            };
            hasher.update(&self.stream[..count]);
            size += u64::try_from(count).expect("a read count fits u64");
        }
        let sha256 = hasher.finish();
        self.rows.insert(path, FileRow { size, sha256 });
        Ok(sha256)
    }

    /// The index of every file taken.
    pub fn finish(self) -> Result<FileIndex, ContentError> {
        FileIndex::new(self.rows)
    }
}
