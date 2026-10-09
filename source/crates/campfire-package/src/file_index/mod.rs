use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use campfire_capabilities::PackagePath;
use campfire_common::Fingerprint;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::ContentError;

/// A package's index: each file's size and SHA-256 by its path, every file of the package but
/// the index itself, as `package.index` holds them after its tag, in postcard, in the order of
/// their paths' bytes. The SHA-256 of those bytes, the tag included, is the package's
/// fingerprint: it names every byte of the package, as git's tree names its blobs, and no two
/// versions of the format share one. A read checks each file against its row.
#[derive(Debug, Clone)]
pub struct FileIndex {
    rows: BTreeMap<PackagePath, FileRow>,
    /// Its one encoding, as `package.index` holds it.
    bytes: Vec<u8>,
    fingerprint: Fingerprint,
}

/// A file's size and SHA-256, as its row lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileRow {
    pub(crate) size: u64,
    pub(crate) sha256: [u8; 32],
}

/// One row as postcard holds it.
#[derive(Debug, Serialize, Deserialize)]
struct WireRow<'a> {
    path: &'a str,
    size: u64,
    sha256: [u8; 32],
}

/// The bytes an index starts with, which name its format and its version.
const INDEX_TAG: &[u8] = b"campfire/package-index/v1";

impl FileIndex {
    /// The index's own path in its package.
    pub const PATH: &str = "package.index";

    /// The index of `rows`, which hold no two paths that differ only in case and not the
    /// index's own path.
    pub(crate) fn new(rows: BTreeMap<PackagePath, FileRow>) -> Result<FileIndex, ContentError> {
        FileIndex::check_spellings(rows.keys())?;
        if rows.keys().any(|path| path.as_str() == FileIndex::PATH) {
            return Err(ContentError::IndexListsItself);
        }
        let bytes = FileIndex::encode_rows(&rows);
        let fingerprint = Fingerprint::new(Sha256::digest(&bytes).into());
        Ok(FileIndex {
            rows,
            bytes,
            fingerprint,
        })
    }

    /// The index `bytes` hold, refused unless they are this version's tag and the one encoding of
    /// rows in strict order of their paths' bytes, each a package path, none the index's own, and
    /// no two that differ only in case.
    pub(crate) fn decode(bytes: &[u8]) -> Result<FileIndex, ContentError> {
        let list = bytes
            .strip_prefix(INDEX_TAG)
            .ok_or(ContentError::IndexTag)?;
        let wire: Vec<WireRow<'_>> =
            postcard::from_bytes(list).map_err(ContentError::IndexDecode)?;
        let mut rows = BTreeMap::new();
        let mut previous: Option<&str> = None;
        for row in &wire {
            let path = PackagePath::parse(row.path)
                .ok_or_else(|| ContentError::IndexPath(row.path.to_owned()))?;
            if previous.is_some_and(|previous| previous.as_bytes() >= row.path.as_bytes()) {
                return Err(ContentError::IndexOrder(path));
            }
            previous = Some(row.path);
            rows.insert(
                path,
                FileRow {
                    size: row.size,
                    sha256: row.sha256,
                },
            );
        }
        let index = FileIndex::new(rows)?;
        // Postcard reads an overlong varint and ignores bytes past the list, so only the one
        // encoding of the rows is an index, and one package has one fingerprint.
        if index.bytes != bytes {
            return Err(ContentError::IndexNotCanonical);
        }
        Ok(index)
    }

    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Its bytes, as `package.index` holds them.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The row of the file at `path`, if the package holds one.
    pub(crate) fn row(&self, path: &PackagePath) -> Option<FileRow> {
        self.rows.get(path).copied()
    }

    /// Every file's path and row, in the order of their paths.
    pub(crate) fn rows(&self) -> impl Iterator<Item = (&PackagePath, FileRow)> {
        self.rows.iter().map(|(path, row)| (path, *row))
    }

    fn encode_rows(rows: &BTreeMap<PackagePath, FileRow>) -> Vec<u8> {
        let wire: Vec<WireRow<'_>> = rows
            .iter()
            .map(|(path, row)| WireRow {
                path: path.as_str(),
                size: row.size,
                sha256: row.sha256,
            })
            .collect();
        postcard::to_extend(&wire, INDEX_TAG.to_vec()).expect("an index always encodes")
    }

    /// Refuses two of `paths`, or two directories on their way, that differ only in case: a
    /// package path is ASCII, so its ASCII lowercase is the one name every file system that
    /// ignores case gives it. Of two spellings, the first in path order is kept and the second
    /// refused.
    fn check_spellings<'a>(
        paths: impl Iterator<Item = &'a PackagePath>,
    ) -> Result<(), ContentError> {
        let mut spellings: BTreeMap<String, &str> = BTreeMap::new();
        for path in paths {
            let text = path.as_str();
            let ends = text.match_indices('/').map(|(at, _)| at);
            for end in ends.chain([text.len()]) {
                let spelled = &text[..end];
                match spellings.entry(spelled.to_ascii_lowercase()) {
                    Entry::Vacant(entry) => {
                        entry.insert(spelled);
                    }
                    Entry::Occupied(entry) if *entry.get() != spelled => {
                        return Err(ContentError::CaseClash {
                            path: path.clone(),
                            other: PackagePath::parse(entry.get())
                                .expect("a path's start is a path"),
                        });
                    }
                    Entry::Occupied(_) => {}
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
