use std::collections::BTreeMap;
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
/// the bytes of each file a load reads, within `limits`. A file no load reads, such as an
/// asset, is hashed as it streams past, and not kept.
#[derive(Debug)]
pub(crate) struct PackageWalk {
    limits: FileLimits,
    /// In the order of their paths, as the fingerprint lists them.
    rows: BTreeMap<PackagePath, FileSum>,
    read: BTreeMap<PackagePath, Vec<u8>>,
    /// The bytes of the files read so far.
    read_bytes: u64,
    /// The buffer a streamed file passes through, made at its first use.
    stream: Vec<u8>,
}

/// How much of a package a walk takes: files in all, the bytes of one file a load reads, and
/// the bytes of all of them, so a package's files cost a bounded memory before a load refuses
/// it. The files no load reads only stream.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FileLimits {
    pub(crate) files: usize,
    pub(crate) file_bytes: u64,
    pub(crate) read_bytes: u64,
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

impl FileLimits {
    /// A package's: 16 384 files, a file a load reads up to 4 MiB, and 16 MiB of them in all.
    /// The reference packages hold about 100 files, the largest under 10 KiB.
    pub(crate) const PACKAGE: FileLimits = FileLimits {
        files: 16_384,
        file_bytes: 4 << 20,
        read_bytes: 16 << 20,
    };
}

impl PackageWalk {
    pub(crate) const fn new(limits: FileLimits) -> PackageWalk {
        PackageWalk {
            limits,
            rows: BTreeMap::new(),
            read: BTreeMap::new(),
            read_bytes: 0,
            stream: Vec::new(),
        }
    }

    /// Whether a load reads the file at `path`: the manifest, and the files of its data, its
    /// map, its scripts and its locales.
    fn reads(path: &PackagePath) -> bool {
        path.as_str() == PackageDir::MANIFEST || READ_DIRS.iter().any(|dir| path.is_under(dir))
    }

    /// Takes the file at `path` whose bytes `file` gives, `size` long as its metadata says,
    /// which may change as it reads: refuses one past the limits before it reads a byte of it.
    pub(crate) fn add(
        &mut self,
        path: PackagePath,
        size: u64,
        file: impl Read,
    ) -> Result<(), ContentError> {
        if self.rows.len() == self.limits.files {
            return Err(ContentError::TooManyFiles);
        }
        let sum = if PackageWalk::reads(&path) {
            self.check_read(&path, size)?;
            let mut bytes = Vec::new();
            file.take(self.limits.file_bytes + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| ContentError::Io {
                    path: path.clone(),
                    error,
                })?;
            let size = u64::try_from(bytes.len()).expect("a file length fits u64");
            self.check_read(&path, size)?;
            self.read_bytes += size;
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

    /// Refuses a file a load reads at `path` of `size` bytes past the limits.
    fn check_read(&self, path: &PackagePath, size: u64) -> Result<(), ContentError> {
        if size > self.limits.file_bytes {
            return Err(ContentError::TooLarge(path.clone()));
        }
        if self.read_bytes + size > self.limits.read_bytes {
            return Err(ContentError::TooMuchToRead);
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
