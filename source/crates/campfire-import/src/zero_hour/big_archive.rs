use std::path::Path;

use campfire_store::{InputRanges, PathError, ReadError};

use crate::error::ImportError;
use crate::zero_hour::error::{ArchiveError, ZeroHourError};

/// A `.big` archive's list of entries, as its header holds it: `BIGF`, its size as a little-endian
/// `u32`, its count of entries and the length of its header as big-endian `u32`s, then each entry's
/// offset and size as big-endian `u32`s and its path, NUL-terminated (`StdBIGFile` in the released
/// source).
#[derive(Debug)]
pub(crate) struct BigArchive {
    /// In the order the header lists them.
    pub(crate) entries: Vec<BigEntry>,
}

/// One file of an archive: its path, as the archive spells it, and where its bytes lie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BigEntry {
    pub(crate) path: String,
    pub(crate) offset: u64,
    pub(crate) size: u64,
}

/// The bytes before the list of entries.
const HEADER: usize = 16;

impl BigArchive {
    /// The entries of the archive at `path`, which `file` opens, each checked to lie within it.
    pub(crate) fn read(file: &mut InputRanges, path: &Path) -> Result<BigArchive, ImportError> {
        let refused = |error| {
            ImportError::ZeroHour(ZeroHourError::Archive {
                archive: path.to_owned(),
                error,
            })
        };
        let short = |error: PathError<ReadError>| match error.error {
            ReadError::Short { .. } => refused(ArchiveError::Short),
            _ => ImportError::Read(error),
        };
        let head = file.read_at(0, HEADER).map_err(short)?;
        if head[..4] != *b"BIGF" {
            return Err(refused(ArchiveError::NotBig));
        }
        let count = be32(&head[8..12]);
        let header_len = usize::try_from(be32(&head[12..16])).expect("a u32 fits usize");
        if header_len < HEADER {
            return Err(refused(ArchiveError::Short));
        }
        let list = file.read_at(0, header_len).map_err(short)?;
        let mut at = HEADER;
        let mut entries = Vec::new();
        for _ in 0..count {
            let fields = list
                .get(at..at + 8)
                .ok_or_else(|| refused(ArchiveError::Short))?;
            let (offset, size) = (u64::from(be32(&fields[..4])), u64::from(be32(&fields[4..])));
            let name = &list[at + 8..];
            let end = name
                .iter()
                .position(|&byte| byte == 0)
                .ok_or_else(|| refused(ArchiveError::Short))?;
            if !name[..end].is_ascii() {
                return Err(refused(ArchiveError::NotAscii));
            }
            let entry = String::from_utf8(name[..end].to_vec()).expect("ASCII is UTF-8");
            if offset + size > file.len() {
                return Err(refused(ArchiveError::EntryPastEnd(entry)));
            }
            entries.push(BigEntry {
                path: entry,
                offset,
                size,
            });
            at += 8 + end + 1;
        }
        Ok(BigArchive { entries })
    }
}

/// The big-endian `u32` of four `bytes`.
fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().expect("four bytes"))
}

#[cfg(test)]
pub(crate) mod internals {
    /// The bytes of a `.big` archive of `files`, each a path and its bytes, in that order, as the
    /// game writes one: the header, the list of entries, then each file's bytes in turn.
    pub(crate) fn big(files: &[(&str, &[u8])]) -> Vec<u8> {
        let list: usize = files.iter().map(|(path, _)| 8 + path.len() + 1).sum();
        let header = 16 + list;
        let total = header + files.iter().map(|(_, bytes)| bytes.len()).sum::<usize>();
        let be = |value: usize| u32::try_from(value).unwrap().to_be_bytes();
        let mut out = b"BIGF".to_vec();
        out.extend(u32::try_from(total).unwrap().to_le_bytes());
        out.extend(be(files.len()));
        out.extend(be(header));
        let mut offset = header;
        for (path, bytes) in files {
            out.extend(be(offset));
            out.extend(be(bytes.len()));
            out.extend(path.as_bytes());
            out.push(0);
            offset += bytes.len();
        }
        for (_, bytes) in files {
            out.extend(*bytes);
        }
        out
    }
}
