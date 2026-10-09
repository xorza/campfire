use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::io::{self, Read};

use campfire_capabilities::PackagePath;
use campfire_common::Fingerprint;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::ContentError;
use crate::package::SCRIPTS;
use crate::package_dir::PackageDir;
use crate::package_files::PackageFiles;
use crate::package_text::LOCALE;

/// One walk over a package's files: each file's row of the file list its fingerprint hashes, and
/// the bytes of each file a load reads. A file no load reads, such as an asset, is hashed as it
/// streams past, and not kept.
#[derive(Debug)]
pub(crate) struct PackageWalk {
    /// In the order of their paths, as the fingerprint lists them.
    rows: BTreeMap<PackagePath, FileSum>,
    read: BTreeMap<PackagePath, Vec<u8>>,
    /// The first spelling of each path and each directory on its way, by its ASCII lowercase,
    /// so two that differ only in case are found.
    spellings: BTreeMap<String, PackagePath>,
    /// The buffer a streamed file passes through, made at its first use.
    stream: Vec<u8>,
}

/// A file's size and SHA-256, as its row lists them.
#[derive(Debug, Clone, Copy)]
struct FileSum {
    size: u64,
    sha256: [u8; 32],
}

/// One row of a package's file list, as its fingerprint hashes it.
#[derive(Debug, Serialize)]
struct FileRow<'a> {
    path: &'a str,
    size: u64,
    sha256: [u8; 32],
}

/// The directories whose files a load reads, beside the manifest.
const READ_DIRS: [&str; 4] = ["data", "map", SCRIPTS, LOCALE];

/// The size of the buffer a streamed file passes through.
const STREAM_CHUNK: usize = 64 * 1024;

impl PackageWalk {
    pub(crate) const fn new() -> PackageWalk {
        PackageWalk {
            rows: BTreeMap::new(),
            read: BTreeMap::new(),
            spellings: BTreeMap::new(),
            stream: Vec::new(),
        }
    }

    /// Whether a load reads the file at `path`: the manifest, and the files of its data, its
    /// map, its scripts and its locales.
    fn reads(path: &PackagePath) -> bool {
        path.as_str() == PackageDir::MANIFEST || READ_DIRS.iter().any(|dir| path.is_under(dir))
    }

    /// Takes the file at `path` whose bytes `file` gives, `size` long as its metadata says: a
    /// file a load reads is read no further than that, so a file that grows as it is read, or a
    /// pipe, ends there, and the bytes it parses are the bytes its row hashes.
    pub(crate) fn add(
        &mut self,
        path: PackagePath,
        size: u64,
        file: impl Read,
    ) -> Result<(), ContentError> {
        self.spell(&path)?;
        let sum = if PackageWalk::reads(&path) {
            let mut bytes = Vec::new();
            file.take(size)
                .read_to_end(&mut bytes)
                .map_err(|error| ContentError::Io {
                    path: path.clone(),
                    error,
                })?;
            let size = u64::try_from(bytes.len()).expect("a file length fits u64");
            let sum = FileSum {
                size,
                sha256: Sha256::digest(&bytes).into(),
            };
            self.read.insert(path.clone(), bytes);
            sum
        } else {
            self.stream(&path, file)?
        };
        self.rows.insert(path, sum);
        Ok(())
    }

    /// Takes the spelling of `path` and of each directory on its way, refusing one that differs
    /// only in case from one taken before: a package path is ASCII, so its ASCII lowercase is
    /// the one name every file system that ignores case gives it.
    fn spell(&mut self, path: &PackagePath) -> Result<(), ContentError> {
        let text = path.as_str();
        let ends = text.match_indices('/').map(|(at, _)| at);
        for end in ends.chain([text.len()]) {
            let spelled = &text[..end];
            match self.spellings.entry(spelled.to_ascii_lowercase()) {
                Entry::Vacant(entry) => {
                    entry.insert(PackagePath::parse(spelled).expect("a path's start is a path"));
                }
                Entry::Occupied(entry) if entry.get().as_str() != spelled => {
                    return Err(ContentError::CaseClash {
                        path: path.clone(),
                        other: entry.get().clone(),
                    });
                }
                Entry::Occupied(_) => {}
            }
        }
        Ok(())
    }

    /// The size and SHA-256 of the bytes of `file`, at `path`, which pass through the buffer.
    fn stream(&mut self, path: &PackagePath, mut file: impl Read) -> Result<FileSum, ContentError> {
        self.stream.resize(STREAM_CHUNK, 0);
        let mut hasher = Sha256::new();
        let mut size = 0_u64;
        loop {
            let count = match file.read(&mut self.stream) {
                Ok(0) => break,
                Ok(count) => count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    return Err(ContentError::Io {
                        path: path.clone(),
                        error,
                    });
                }
            };
            hasher.update(&self.stream[..count]);
            size += u64::try_from(count).expect("a read count fits u64");
        }
        Ok(FileSum {
            size,
            sha256: hasher.finalize().into(),
        })
    }

    /// The files a load reads, and the fingerprint of every file: the SHA-256 of the postcard
    /// list of each file's path, size and SHA-256, in the order of their paths.
    pub(crate) fn finish(self) -> PackageFiles {
        let rows: Vec<FileRow<'_>> = self
            .rows
            .iter()
            .map(|(path, sum)| FileRow {
                path: path.as_str(),
                size: sum.size,
                sha256: sum.sha256,
            })
            .collect();
        let list = postcard::to_allocvec(&rows).expect("a file list always encodes");
        let fingerprint = Fingerprint::new(Sha256::digest(list).into());
        PackageFiles::new(self.read, fingerprint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_file_is_taken_to_its_size_and_none_is_too_large() {
        // A file that grew past the 2 bytes its metadata gave as it opened: the walk keeps those 2
        // and its row hashes them, as a walk of the 2 alone does.
        let data = PackagePath::parse("data/a.toml").unwrap();
        let mut grown = PackageWalk::new();
        grown.add(data.clone(), 2, b"a=1".as_slice()).unwrap();
        let mut exact = PackageWalk::new();
        exact.add(data.clone(), 2, b"a=".as_slice()).unwrap();
        let (grown, exact) = (grown.finish(), exact.finish());
        assert_eq!(grown.read_text(&data).unwrap(), "a=");
        assert_eq!(grown.fingerprint(), exact.fingerprint());

        // Four files a load reads of 5 MiB each, 20 MiB in all: each reads whole, as a package has
        // no limit of bytes.
        let big = vec![b'#'; 5 << 20];
        let mut walk = PackageWalk::new();
        let paths = ["data/b.toml", "data/c.toml", "data/d.toml", "data/e.toml"]
            .map(|path| PackagePath::parse(path).unwrap());
        for path in &paths {
            walk.add(path.clone(), 5 << 20, big.as_slice()).unwrap();
        }
        let files = walk.finish();
        for path in &paths {
            assert_eq!(files.read_text(path).unwrap().len(), 5 << 20);
        }
    }
}
