use std::collections::BTreeMap;
use std::io::{self, Read};

use campfire_capabilities::PackagePath;
use sha2::{Digest, Sha256};

use crate::error::ContentError;
use crate::file_index::{FileIndex, FileRow};

/// One walk over a package's files, as a package is packed: each file's size and SHA-256, hashed
/// as it streams past and not kept, into the package's index.
#[derive(Debug)]
pub(crate) struct PackageWalk {
    rows: BTreeMap<PackagePath, FileRow>,
    /// The buffer a file passes through, made at its first use.
    stream: Vec<u8>,
}

/// The size of the buffer a file passes through.
const STREAM_CHUNK: usize = 64 * 1024;

impl PackageWalk {
    pub(crate) const fn new() -> PackageWalk {
        PackageWalk {
            rows: BTreeMap::new(),
            stream: Vec::new(),
        }
    }

    /// Takes the file at `path` whose bytes `file` gives, to its end.
    pub(crate) fn add(
        &mut self,
        path: PackagePath,
        mut file: impl Read,
    ) -> Result<(), ContentError> {
        self.stream.resize(STREAM_CHUNK, 0);
        let mut hasher = Sha256::new();
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
        self.rows.insert(
            path,
            FileRow {
                size,
                sha256: hasher.finalize().into(),
            },
        );
        Ok(())
    }

    /// The index of every file taken.
    pub(crate) fn finish(self) -> Result<FileIndex, ContentError> {
        FileIndex::new(self.rows)
    }
}
