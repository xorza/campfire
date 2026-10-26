use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::io::{self, Read};

use crate::content_hash::{ContentHash, ContentHashMeaning, ContentHasher};
use campfire_values::PackagePath;

use crate::error::ContentError;
use crate::file_index::{EncodedIndex, FileIndex, FileRow};

/// One walk over a package's files, as a package is packed: each file's size and SHA-256, hashed
/// as it streams past and not kept, into the package's index. A path it took before is refused.
#[derive(Debug, Default)]
pub struct PackageWalk {
    rows: BTreeMap<PackagePath, FileRow>,
    /// The buffer a file passes through, made at its first use.
    stream: Vec<u8>,
}

/// A file's size and SHA-256, as a walk hashed it, for the walk to take as a path's row once
/// the file is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashedFile(FileRow);

/// The size of the buffer a file passes through.
const STREAM_CHUNK: usize = 64 * 1024;

impl PackageWalk {
    pub const fn new() -> Self {
        Self {
            rows: BTreeMap::new(),
            stream: Vec::new(),
        }
    }

    /// The size and SHA-256 of the bytes `file` gives, to its end, which no path takes yet; the
    /// reader's own error, for its owner to name, as a failed read is no flaw of the package.
    pub fn hash(&mut self, mut file: impl Read) -> io::Result<HashedFile> {
        self.stream.resize(STREAM_CHUNK, 0);
        let mut hasher = ContentHasher::<ContentHashMeaning>::new();
        let mut size = 0_u64;
        loop {
            let count = match file.read(&mut self.stream) {
                Ok(0) => break,
                Ok(count) => count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            hasher.update(&self.stream[..count]);
            size += u64::try_from(count).expect("a read count fits u64");
        }
        Ok(HashedFile(FileRow {
            size,
            sha256: hasher.finish(),
        }))
    }

    /// Takes `hashed` as the row of `path`, a path it took no row of.
    pub fn insert(&mut self, path: PackagePath, hashed: HashedFile) -> Result<(), ContentError> {
        match self.rows.entry(path) {
            Entry::Vacant(row) => {
                row.insert(hashed.0);
                Ok(())
            }
            Entry::Occupied(row) => Err(ContentError::Repeated(row.key().clone())),
        }
    }

    /// Whether it holds the row of a file at `path`.
    pub fn contains(&self, path: &PackagePath) -> bool {
        self.rows.contains_key(path)
    }

    /// The index of every file taken, and its encoding.
    pub fn finish(self) -> Result<EncodedIndex, ContentError> {
        FileIndex::encode(self.rows)
    }
}

impl HashedFile {
    /// The size and SHA-256 of `bytes`, a file held whole, which needs no buffer to pass through.
    pub fn of(bytes: &[u8]) -> Self {
        let mut hasher = ContentHasher::<ContentHashMeaning>::new();
        hasher.update(bytes);
        Self(FileRow {
            size: u64::try_from(bytes.len()).expect("a length fits u64"),
            sha256: hasher.finish(),
        })
    }

    pub const fn sha256(self) -> ContentHash {
        self.0.sha256
    }
}
