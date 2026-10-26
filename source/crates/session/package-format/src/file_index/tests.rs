use std::io;

use super::*;
use crate::package_walk::{HashedFile, PackageWalk};

/// An index's bytes: its tag's line, then `rows`, each a line.
fn indexed(rows: &[&str]) -> Vec<u8> {
    let mut text = format!("{INDEX_TAG}\n");
    for row in rows {
        text.push_str(row);
        text.push('\n');
    }
    text.into_bytes()
}

/// A reader that gives each of its reads in turn, the first first, then its end.
struct Reads(Vec<Result<&'static [u8], io::ErrorKind>>);

impl io::Read for Reads {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.0.is_empty() {
            return Ok(0);
        }
        let bytes = self.0.remove(0).map_err(io::Error::from)?;
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(bytes.len())
    }
}

/// Takes the file at `path` of `bytes` into `walk`.
fn add(walk: &mut PackageWalk, path: PackagePath, bytes: &[u8]) -> Result<(), ContentError> {
    let hashed = walk.hash(bytes).unwrap();
    walk.insert(path, hashed)
}

/// A walk that takes `path` of "abc", then again of `bytes`: the second's answer.
fn walk_twice(path: &PackagePath, bytes: &[u8]) -> Result<(), ContentError> {
    let mut walk = PackageWalk::default();
    add(&mut walk, path.clone(), b"abc").unwrap();
    let again = add(&mut walk, path.clone(), bytes);
    assert_eq!(walk.finish().unwrap().index.row(path).unwrap().size, 3);
    again
}

/// One row's line: its SHA-256 of 32 bytes of `byte`, as 64 hex digits, its size and its path.
fn row(byte: u8, size: u64, path: &str) -> String {
    format!("{} {size} {path}", ContentHash::new([byte; 32]))
}

#[test]
fn an_index_is_its_one_encoding_of_rows_in_path_order() {
    let a = row(0x1a, 2, "data/a.toml");
    let m = row(0x02, 1, "manifest.toml");
    // Two rows in order, as their lines: `1a1a…1a 2 data/a.toml` and `0202…02 1 manifest.toml`.
    let bytes = indexed(&[&a, &m]);
    assert_eq!(&a[..6], "1a1a1a");
    assert_eq!(&a[62..], "1a 2 data/a.toml");
    let index = FileIndex::decode(&bytes).unwrap();
    assert_eq!(index.fingerprint(), PackageFingerprint::of(&bytes));
    let data = PackagePath::parse("data/a.toml").unwrap();
    assert_eq!(
        index.row(&data),
        Some(FileRow {
            size: 2,
            sha256: ContentHash::new([0x1a; 32])
        })
    );
    assert_eq!(index.row(&PackagePath::parse("data/b.toml").unwrap()), None);
    // A path holding a space reads whole, as the rest of its line.
    let spaced = FileIndex::decode(&indexed(&[&row(0, 0, "a b/c d.txt")])).unwrap();
    let path = PackagePath::parse("a b/c d.txt").unwrap();
    assert_eq!(spaced.row(&path).map(|row| row.size), Some(0));
    // A row of a file of 2 GiB, the most a package's file holds, decodes; one byte more is
    // refused, as its read would allocate it.
    let sized = |size: u64| FileIndex::decode(&indexed(&[&row(0, size, "big.bin")]));
    assert!(sized(FileIndex::MAX_FILE_LEN).is_ok());
    assert!(matches!(
        sized(FileIndex::MAX_FILE_LEN + 1),
        Err(ContentError::IndexFileLen(path)) if path.as_str() == "big.bin"
    ));
    // An index is untrusted: one cut short within a line is refused, one cut at a line's end is
    // an index of fewer rows, of another fingerprint, and no flip of a byte panics.
    for at in 0..bytes.len() {
        let cut = FileIndex::decode(&bytes[..at]);
        if at == INDEX_TAG.len() + 1 || at == INDEX_TAG.len() + 2 + a.len() {
            assert_ne!(cut.unwrap().fingerprint(), index.fingerprint());
        } else {
            assert!(cut.is_err(), "cut to {at} bytes");
        }
        for flip in [0x01, 0x80, 0xFF] {
            let mut flawed = bytes.clone();
            flawed[at] ^= flip;
            FileIndex::decode(&flawed).ok();
        }
    }
    // A row of a file of "abc", as a walk takes it: its 3 bytes, and SHA-256's own test vector.
    let path = PackagePath::parse("abc.txt").unwrap();
    let mut walk = PackageWalk::default();
    add(&mut walk, path.clone(), b"abc").unwrap();
    let abc = walk.finish().unwrap();
    assert_eq!(abc.index.row(&path).unwrap().size, 3);
    let written = String::from_utf8(abc.bytes).unwrap();
    assert_eq!(
        written,
        "campfire/package-index/v2\n\
         ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad 3 abc.txt\n",
        "SHA-256(\"abc\") is ba7816bf…"
    );
    // A walk refuses a path it took before, of the same bytes or others, and keeps its first row.
    for bytes in [&b"abc"[..], b"x"] {
        assert!(matches!(
            walk_twice(&path, bytes),
            Err(ContentError::Repeated(repeated)) if repeated == path
        ));
    }
    // A read interrupted goes on, and one that fails gives the reader's own error, no row of
    // the package's.
    let mut walk = PackageWalk::default();
    let interrupted = walk
        .hash(Reads(vec![Err(io::ErrorKind::Interrupted), Ok(b"abc")]))
        .unwrap();
    assert_eq!(interrupted, walk.hash(&b"abc"[..]).unwrap());
    // A file held whole hashes as its stream does: none, one byte, and one chunk of 64 KiB and a
    // byte more, which streams in two reads.
    for bytes in [Vec::new(), vec![7], vec![7; 64 * 1024 + 1]] {
        assert_eq!(HashedFile::of(&bytes), walk.hash(bytes.as_slice()).unwrap());
    }
    let failed = walk.hash(Reads(vec![Ok(b"ab"), Err(io::ErrorKind::PermissionDenied)]));
    assert_eq!(failed.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(walk.finish().unwrap().index.rows().count(), 0);
    // No rows is a package of no file.
    assert_eq!(FileIndex::decode(&indexed(&[])).unwrap().rows().count(), 0);
}

#[test]
fn an_index_refuses_every_other_spelling_of_its_rows() {
    let a = row(0x1a, 2, "data/a.toml");
    let m = row(0x02, 1, "manifest.toml");
    // No tag, another version's, the tag with no line end, or a byte no text holds before it.
    let v1 = [&b"campfire/package-index/v1"[..], &[0]].concat();
    for bytes in [vec![], v1, INDEX_TAG.as_bytes().to_vec(), vec![0xFF, b'\n']] {
        assert!(matches!(
            FileIndex::decode(&bytes),
            Err(ContentError::IndexTag)
        ));
    }
    // Out of order, or the same path twice: the second row is refused.
    for rows in [[m.as_str(), a.as_str()], [a.as_str(), a.as_str()]] {
        assert!(matches!(
            FileIndex::decode(&indexed(&rows)),
            Err(ContentError::IndexOrder(_))
        ));
    }
    // Each line but the tag's that spells a row another way is refused by its number: the hex
    // in uppercase, or one digit short; the size with a leading zero, a sign or none, or past a
    // `u64`; two spaces between fields; an empty line; and the last line with no LF.
    let hex = ContentHash::new([0x1a; 32]).to_string();
    let flawed = [
        format!("{} 2 data/a.toml", hex.to_uppercase()),
        format!("{} 2 data/a.toml", &hex[1..]),
        format!("{hex} 02 data/a.toml"),
        format!("{hex} +2 data/a.toml"),
        format!("{hex}  data/a.toml"),
        format!("{hex} 18446744073709551616 data/a.toml"),
        format!("{hex}  2 data/a.toml"),
        String::new(),
    ];
    for line in &flawed {
        assert!(
            matches!(
                FileIndex::decode(&indexed(&[line])),
                Err(ContentError::IndexLine { line: 2 })
            ),
            "{line:?}"
        );
    }
    let unended = [INDEX_TAG.as_bytes(), b"\n", a.as_bytes()].concat();
    assert!(matches!(
        FileIndex::decode(&unended),
        Err(ContentError::IndexLine { line: 2 })
    ));
    // A byte no text holds, in the path of the third row, is refused by its line.
    let mut unreadable = indexed(&[&a, &m, &row(0, 0, "x")]);
    let last = unreadable.len() - 2;
    unreadable[last] = 0xFF;
    assert!(matches!(
        FileIndex::decode(&unreadable),
        Err(ContentError::IndexLine { line: 4 })
    ));
    let third = indexed(&[&a, &m, "x"]);
    assert!(matches!(
        FileIndex::decode(&third),
        Err(ContentError::IndexLine { line: 4 })
    ));
    // A CR before a line's LF is part of its path, which no package names.
    let crlf = [INDEX_TAG.as_bytes(), b"\n", m.as_bytes(), b"\r\n"].concat();
    assert!(matches!(
        FileIndex::decode(&crlf),
        Err(ContentError::IndexPath(path)) if path == "manifest.toml\r"
    ));
    // A path no package names, the index's own path, and two that differ only in case.
    let refused = |path: &str| FileIndex::decode(&indexed(&[&row(0, 0, path)]));
    assert!(matches!(refused("../x"), Err(ContentError::IndexPath(path)) if path == "../x"));
    assert!(matches!(
        refused("package.index"),
        Err(ContentError::IndexListsItself)
    ));
    let clash = indexed(&[&row(0, 0, "Data/b"), &row(0, 0, "data/a")]);
    assert!(matches!(
        FileIndex::decode(&clash),
        Err(ContentError::CaseClash { path, other })
            if path.as_str() == "data/a" && other.as_str() == "Data"
    ));
    // The index's own path in another case, and a path under it, which a file system that
    // ignores case holds where the index goes.
    for path in [
        "PACKAGE.INDEX",
        "Package.Index/a.toml",
        "package.index/a.toml",
    ] {
        assert!(
            matches!(refused(path), Err(ContentError::IndexListsItself)),
            "{path}"
        );
    }
    // A file `data` and a file under a directory `data`, which no file system holds.
    let both = indexed(&[&row(0, 0, "data"), &row(0, 0, "data/a")]);
    assert!(matches!(
        FileIndex::decode(&both),
        Err(ContentError::FileAndDir { path, name })
            if path.as_str() == "data/a" && name.as_str() == "data"
    ));
}

#[test]
fn an_index_a_builder_makes_holds_every_rule_its_read_holds() {
    let rows = |listed: &[(&str, u64)]| -> BTreeMap<PackagePath, FileRow> {
        listed
            .iter()
            .map(|&(path, size)| {
                let row = FileRow {
                    size,
                    sha256: ContentHash::new([0; 32]),
                };
                (PackagePath::parse(path).unwrap(), row)
            })
            .collect()
    };
    // A walk that takes the index's own path in another case, or a file and a directory of one
    // name, makes no index.
    for refused in [
        vec![PackagePath::parse("Package.Index").unwrap()],
        vec![
            PackagePath::parse("data").unwrap(),
            PackagePath::parse("data/a").unwrap(),
        ],
    ] {
        let mut walk = PackageWalk::default();
        for path in refused {
            add(&mut walk, path, b"").unwrap();
        }
        assert!(walk.finish().is_err());
    }
    // A file of 2 GiB, the most a package's file holds, is a row; one byte more is refused.
    assert!(FileIndex::encode(rows(&[("big.bin", FileIndex::MAX_FILE_LEN)])).is_ok());
    assert!(matches!(
        FileIndex::encode(rows(&[("big.bin", FileIndex::MAX_FILE_LEN + 1)])),
        Err(ContentError::IndexFileLen(path)) if path.as_str() == "big.bin"
    ));
    // Rows of one name of 65,437 bytes each: a row's line is 64 hex digits, a space, its size of
    // one digit, a space, the name and LF, 65,505 bytes, and the tag's line 26, so 1,024 rows are
    // 26 + 1,024 × 65,505 = 67,077,146 bytes, within the 67,108,864 of `MAX_LEN`, and 1,025 are
    // 67,142,651, past it.
    let named = |at: usize| format!("{at:05}{}", "x".repeat(65_432));
    let listed =
        |count: usize| -> Vec<(String, u64)> { (0..count).map(|at| (named(at), 0)).collect() };
    let index = |count: usize| {
        let listed = listed(count);
        let listed: Vec<(&str, u64)> = listed
            .iter()
            .map(|(path, size)| (path.as_str(), *size))
            .collect();
        FileIndex::encode(rows(&listed))
    };
    assert_eq!(index(1024).unwrap().bytes.len(), 26 + 1024 * 65_505);
    assert!(matches!(index(1025), Err(ContentError::IndexLen)));
}
