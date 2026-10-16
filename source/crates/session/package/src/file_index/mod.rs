use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt::Write as _;

use campfire_common::{ContentHash, PackageFingerprint};
use campfire_values::PackagePath;

use crate::error::ContentError;

/// A package's index: each file's size and SHA-256 by its path, every file of the package but
/// the index itself, as `package.index` holds them: the line of its tag, then a line for each
/// file in the order of their paths' bytes, `<SHA-256> <size> <path>`, the SHA-256 in 64
/// lowercase hex digits, the size in decimal digits with no leading zero, each line ending in
/// LF. The SHA-256 of those bytes, the tag included, is the package's fingerprint: it names every
/// byte of the package, as git's tree names its blobs, and no two versions of the format share
/// one. A read checks each file against its row.
#[derive(Debug, Clone)]
pub struct FileIndex {
    rows: BTreeMap<PackagePath, FileRow>,
    /// Its one encoding, as `package.index` holds it.
    bytes: Vec<u8>,
    fingerprint: PackageFingerprint,
}

/// A file's size and SHA-256, as its row lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileRow {
    pub(crate) size: u64,
    pub(crate) sha256: ContentHash,
}

/// The first line of an index, which names its format and its version.
const INDEX_TAG: &str = "campfire/package-index/v2";

impl FileIndex {
    /// The index's own path in its package.
    pub const PATH: &str = "package.index";

    /// The most bytes an index holds, which its read takes at most: 64 MiB, the rows of about
    /// 240 000 files whose paths are 200 bytes long.
    pub const MAX_LEN: u64 = 1 << 26;

    /// The most bytes a file of a package holds, which its read allocates: 2 GiB, past any
    /// texture or model an importer writes.
    pub const MAX_FILE_LEN: u64 = 1 << 31;

    /// The index of `rows`, which hold no two paths that differ only in case and not the
    /// index's own path, as a package's builder makes it.
    pub(crate) fn new(rows: BTreeMap<PackagePath, FileRow>) -> Result<Self, ContentError> {
        Self::check_spellings(rows.keys())?;
        if rows.keys().any(|path| path.as_str() == Self::PATH) {
            return Err(ContentError::IndexListsItself);
        }
        let bytes = Self::encode_rows(&rows);
        let fingerprint = PackageFingerprint::of(&bytes);
        Ok(Self {
            rows,
            bytes,
            fingerprint,
        })
    }

    /// The index `bytes` hold, refused unless they are this version's tag and the one encoding of
    /// rows in strict order of their paths' bytes, each a package path, none the index's own, of
    /// a file no larger than `MAX_FILE_LEN`, and no two that differ only in case.
    pub fn decode(bytes: &[u8]) -> Result<Self, ContentError> {
        let body = bytes
            .strip_prefix(INDEX_TAG.as_bytes())
            .and_then(|rest| rest.strip_prefix(b"\n"))
            .ok_or(ContentError::IndexTag)?;
        let body = str::from_utf8(body).map_err(|error| {
            let before = str::from_utf8(&body[..error.valid_up_to()]).expect("text up to there");
            let line = before.matches('\n').count() + 2;
            ContentError::IndexLine { line }
        })?;
        let mut rows = BTreeMap::new();
        let mut previous: Option<&str> = None;
        for (line, number) in body.split_inclusive('\n').zip(2_usize..) {
            let flawed = || ContentError::IndexLine { line: number };
            let line = line.strip_suffix('\n').ok_or_else(flawed)?;
            let (sha256, rest) = line.split_once(' ').ok_or_else(flawed)?;
            let (size, path) = rest.split_once(' ').ok_or_else(flawed)?;
            let sha256 = sha256.parse::<ContentHash>().map_err(|_| flawed())?;
            let size = Self::size(size).ok_or_else(flawed)?;
            let parsed =
                PackagePath::parse(path).ok_or_else(|| ContentError::IndexPath(path.to_owned()))?;
            if previous.is_some_and(|previous| previous.as_bytes() >= path.as_bytes()) {
                return Err(ContentError::IndexOrder(parsed));
            }
            previous = Some(path);
            if size > Self::MAX_FILE_LEN {
                return Err(ContentError::IndexFileLen(parsed));
            }
            rows.insert(parsed, FileRow { size, sha256 });
        }
        let index = Self::new(rows)?;
        debug_assert_eq!(
            index.bytes, bytes,
            "an index reads from its one encoding alone"
        );
        Ok(index)
    }

    /// The size `digits` write in decimal, with no sign and no leading zero; none for any other
    /// text, or one past a `u64`.
    fn size(digits: &str) -> Option<u64> {
        let decimal = !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit());
        let leading = digits.len() > 1 && digits.starts_with('0');
        if !decimal || leading {
            return None;
        }
        digits.parse().ok()
    }

    pub const fn fingerprint(&self) -> PackageFingerprint {
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
        let mut text = String::new();
        writeln!(text, "{INDEX_TAG}").expect("text writes into a string");
        for (path, row) in rows {
            writeln!(text, "{} {} {path}", row.sha256, row.size)
                .expect("text writes into a string");
        }
        text.into_bytes()
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
