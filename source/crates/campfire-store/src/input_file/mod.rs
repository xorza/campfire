use std::fs::{self, File, Metadata};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::input_file::error::ReadError;
use crate::path_error::PathError;

pub(crate) mod error;

/// A file read never past a bound its caller names: whole, within a bound the owner of its format
/// derives from that format's own limits; in a stream, which its reader bounds; or by ranges, each
/// within its own. A whole read reserves the length its open handle's metadata gives, once, and
/// reads at most one byte past the bound, so a file that grows as it is read is refused, not read
/// without end.
#[derive(Debug)]
pub struct InputFile;

/// A regular file open to read in pieces, as a walk hashes a file it does not keep: its length,
/// as its handle's metadata gave it as it opened, and its bytes, as `Read` gives them.
#[derive(Debug)]
pub struct InputStream {
    len: u64,
    file: File,
}

/// A regular file open to read ranges of, as an archive's entries are read: its stamp, as its
/// handle's metadata gave it as it opened, and the bytes of each range, all from the one file it
/// opened, so a file swapped in between two reads mixes nothing.
#[derive(Debug)]
pub struct InputRanges {
    path: PathBuf,
    stamp: FileStamp,
    file: File,
}

/// A file's length and time of change, as its open handle's metadata gives them: a reader that
/// opens a file again compares them to know it is the file it read before, as `make` and `git`
/// do. A change that keeps both is not seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub len: u64,
    pub modified: SystemTime,
}

/// A file's bytes and its time of change, both from one open handle.
#[derive(Debug)]
pub struct Stamped {
    pub bytes: Vec<u8>,
    pub modified: SystemTime,
}

/// A regular file, open, and its metadata, read from its handle so the file checked is the file
/// read.
#[derive(Debug)]
pub(crate) struct Opened {
    pub(crate) file: File,
    pub(crate) metadata: Metadata,
}

impl InputFile {
    /// The bytes of the file at `path`, at most `max_len`.
    pub fn read(path: &Path, max_len: usize) -> Result<Vec<u8>, PathError<ReadError>> {
        InputFile::open(path)
            .and_then(|opened| opened.read(max_len))
            .map_err(PathError::at(path))
    }

    /// The bytes of the file at `path`, at most `max_len`; none when there is no file.
    pub fn read_if_present(
        path: &Path,
        max_len: usize,
    ) -> Result<Option<Vec<u8>>, PathError<ReadError>> {
        match InputFile::open(path).and_then(|opened| opened.read(max_len)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(ReadError::Missing) => Ok(None),
            Err(error) => Err(PathError::at(path)(error)),
        }
    }

    /// The UTF-8 text of the file at `path`, at most `max_len` bytes.
    pub fn read_text(path: &Path, max_len: usize) -> Result<String, PathError<ReadError>> {
        let bytes = InputFile::read(path, max_len)?;
        String::from_utf8(bytes)
            .map_err(|error| PathError::at(path)(ReadError::NotText(error.utf8_error())))
    }

    /// The bytes of the file at `path`, at most `max_len`, and its time of change, both from one
    /// open handle.
    pub fn read_stamped(path: &Path, max_len: usize) -> Result<Stamped, PathError<ReadError>> {
        let read = || {
            let opened = InputFile::open(path)?;
            let modified = opened.metadata.modified().map_err(ReadError::Read)?;
            let bytes = opened.read(max_len)?;
            Ok(Stamped { bytes, modified })
        };
        read().map_err(PathError::at(path))
    }

    /// The regular file at `path`, open to read in pieces; its reader bounds what it takes.
    pub fn stream(path: &Path) -> Result<InputStream, PathError<ReadError>> {
        let Opened { file, metadata } = InputFile::open(path).map_err(PathError::at(path))?;
        Ok(InputStream {
            len: metadata.len(),
            file,
        })
    }

    /// The regular file at `path`, open to read ranges of; each read is bounded by its range.
    pub fn ranges(path: &Path) -> Result<InputRanges, PathError<ReadError>> {
        let open = || {
            let Opened { file, metadata } = InputFile::open(path)?;
            let modified = metadata.modified().map_err(ReadError::Read)?;
            Ok(InputRanges {
                path: path.to_owned(),
                stamp: FileStamp {
                    len: metadata.len(),
                    modified,
                },
                file,
            })
        };
        open().map_err(PathError::at(path))
    }

    /// The regular file at `path`, open, with its metadata. The path's kind is checked before
    /// it opens, as PostgreSQL checks a key file's: an open of a pipe waits for a writer, and
    /// Windows opens no directory as a file. The handle's own kind is checked again after, so a
    /// file swapped in between is still refused.
    pub(crate) fn open(path: &Path) -> Result<Opened, ReadError> {
        let missing = |error: io::Error| {
            if error.kind() == io::ErrorKind::NotFound {
                ReadError::Missing
            } else {
                ReadError::Read(error)
            }
        };
        if !fs::metadata(path).map_err(missing)?.is_file() {
            return Err(ReadError::NotFile);
        }
        let file = File::open(path).map_err(missing)?;
        let metadata = file.metadata().map_err(ReadError::Read)?;
        if !metadata.is_file() {
            return Err(ReadError::NotFile);
        }
        Ok(Opened { file, metadata })
    }
}

impl InputStream {
    /// Its length as it opened.
    pub const fn len(&self) -> u64 {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl InputRanges {
    /// Its length as it opened.
    pub const fn len(&self) -> u64 {
        self.stamp.len
    }

    pub const fn is_empty(&self) -> bool {
        self.stamp.len == 0
    }

    /// Its length and time of change as it opened.
    pub const fn stamp(&self) -> FileStamp {
        self.stamp
    }

    /// The `len` bytes from `offset`. A range past the length it opened with is `Short` before
    /// it reads, and so is one the file no longer holds as it reads, as when it shrank.
    pub fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, PathError<ReadError>> {
        let short = || ReadError::Short { offset, len };
        let wanted = u64::try_from(len).expect("a length fits u64");
        let read = || {
            if offset
                .checked_add(wanted)
                .is_none_or(|end| end > self.stamp.len)
            {
                return Err(short());
            }
            (&self.file)
                .seek(SeekFrom::Start(offset))
                .map_err(ReadError::Read)?;
            let mut bytes = Vec::with_capacity(len);
            (&self.file)
                .take(wanted)
                .read_to_end(&mut bytes)
                .map_err(ReadError::Read)?;
            if bytes.len() < len {
                return Err(short());
            }
            Ok(bytes)
        };
        read().map_err(PathError::at(&self.path))
    }
}

impl Read for InputStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Opened {
    /// Whether its metadata gives it more than `max_len` bytes; a length past `usize` is past any
    /// bound.
    pub(crate) fn longer_than(&self, max_len: usize) -> bool {
        usize::try_from(self.metadata.len()).map_or(true, |len| len > max_len)
    }

    /// Its bytes, at most `max_len`, in a buffer of the length its metadata gives.
    fn read(self, max_len: usize) -> Result<Vec<u8>, ReadError> {
        let too_large = || ReadError::TooLarge { max: max_len };
        if self.longer_than(max_len) {
            return Err(too_large());
        }
        let len = usize::try_from(self.metadata.len()).expect("a length within the bound");
        let mut bytes = Vec::with_capacity(len + 1);
        self.file
            .take(u64::try_from(max_len).expect("a length fits u64") + 1)
            .read_to_end(&mut bytes)
            .map_err(ReadError::Read)?;
        if bytes.len() > max_len {
            return Err(too_large());
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests;
