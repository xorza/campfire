use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use campfire_common::{ContentHash, PackageFingerprint};
use campfire_package::{FileIndex, PackageWalk};
use campfire_store::{DurableDir, DurableFile, OutputFile};
use campfire_values::PackagePath;

use crate::error::WriteError;

/// A new package as its builder writes it, an importer or a packer: each file into a new
/// directory, written once with no sync and its row kept, then the index of the rows, synced, so
/// every file is checked against what was written whenever it is read, and a write a crash cut
/// short is found then. A file whose SHA-256 an earlier file has is a hard link to that file, so
/// the package keeps one copy of identical files on disk (05-protocol-spec.md, Packages).
#[derive(Debug)]
pub struct PackageWriter {
    root: PathBuf,
    rows: PackageWalk,
    /// The first file written of each SHA-256.
    firsts: BTreeMap<ContentHash, PathBuf>,
}

impl PackageWriter {
    /// A writer of the package in the new directory `root`; an error when one is there.
    pub fn create(root: &Path) -> Result<Self, WriteError> {
        DurableDir::create_new(root).map_err(WriteError::CreateDir)?;
        Ok(Self {
            root: root.to_owned(),
            rows: PackageWalk::new(),
            firsts: BTreeMap::new(),
        })
    }

    /// Writes the file at `path` with `bytes`, and the directories on its way, linked to the
    /// first file of the same bytes; an error for a path written before. The row and the link's
    /// target are kept once the file is written, so a write refused leaves neither.
    pub fn write(&mut self, path: PackagePath, bytes: &[u8]) -> Result<(), WriteError> {
        let at = path
            .as_str()
            .split('/')
            .fold(self.root.clone(), |at, name| at.join(name));
        if let Some(dir) = at.parent() {
            DurableDir::create_all(dir).map_err(WriteError::Directory)?;
        }
        let hashed = self.rows.hash(bytes).expect("a slice reads");
        match self.firsts.get(&hashed.sha256()) {
            Some(first) => OutputFile::link_new(&at, first, bytes),
            None => OutputFile::write_new(&at, bytes),
        }
        .map_err(WriteError::File)?;
        self.rows
            .insert(path, hashed)
            .map_err(WriteError::Package)?;
        self.firsts.entry(hashed.sha256()).or_insert(at);
        Ok(())
    }

    /// Writes the package's index; its fingerprint.
    pub fn finish(self) -> Result<PackageFingerprint, WriteError> {
        let encoded = self.rows.finish().map_err(WriteError::Package)?;
        DurableFile::write(&self.root.join(FileIndex::PATH), &encoded.bytes)
            .map_err(WriteError::Index)?;
        Ok(encoded.index.fingerprint())
    }
}

#[cfg(test)]
mod tests {
    use campfire_store::internals::TestDir;

    use super::*;
    use crate::package_dir::PackageDir;

    fn path(text: &str) -> PackagePath {
        PackagePath::parse(text).unwrap()
    }

    #[test]
    fn a_writer_writes_each_file_and_their_index_in_path_order() {
        let test_dir = TestDir::new();
        let write = |dir: &str, order: [(&str, &[u8]); 2]| {
            let mut writer = PackageWriter::create(&test_dir.path(dir)).unwrap();
            for (at, bytes) in order {
                writer.write(path(at), bytes).unwrap();
            }
            writer.finish().unwrap()
        };
        let fingerprint = write("one", [("data/b.toml", b"b"), ("a.txt", b"aa")]);
        assert_eq!(test_dir.read("one/data/b.toml"), b"b");
        assert_eq!(test_dir.read("one/a.txt"), b"aa");
        // The tag's line, then the rows in path order: `a.txt` before `data/b.toml`.
        let hex = ContentHash::of;
        let index = format!(
            "campfire/package-index/v2\n{} 2 a.txt\n{} 1 data/b.toml\n",
            hex(b"aa"),
            hex(b"b")
        )
        .into_bytes();
        assert_eq!(test_dir.read("one/package.index"), index);
        assert_eq!(fingerprint, PackageFingerprint::of(&index));
        assert_eq!(
            PackageDir::new(test_dir.path("one"))
                .files()
                .unwrap()
                .fingerprint(),
            fingerprint
        );
        // The same files written in another order are the same package.
        assert_eq!(
            write("two", [("a.txt", b"aa"), ("data/b.toml", b"b")]),
            fingerprint
        );
        assert_eq!(test_dir.read("two/package.index"), index);

        // A file of the bytes of one written before is a link to that file, the first of them in
        // the order of writing: one file on disk, which each path reads whole; the index lists
        // each path, as a writer of copies would.
        let mut linked = PackageWriter::create(&test_dir.path("four")).unwrap();
        for (at, bytes) in [
            ("data/b.toml", &b"b"[..]),
            ("a.txt", b"aa"),
            ("c/d.txt", b"aa"),
        ] {
            linked.write(path(at), bytes).unwrap();
        }
        let fingerprint = linked.finish().unwrap();
        assert_eq!(test_dir.read("four/c/d.txt"), b"aa");
        assert!(test_dir.same_file("four/a.txt", "four/c/d.txt"));
        assert!(!test_dir.same_file("four/a.txt", "four/data/b.toml"));
        let rows = format!(
            "campfire/package-index/v2\n{aa} 2 a.txt\n{aa} 2 c/d.txt\n{} 1 data/b.toml\n",
            hex(b"b"),
            aa = hex(b"aa")
        );
        assert_eq!(test_dir.read("four/package.index"), rows.as_bytes());
        let files = PackageDir::new(test_dir.path("four")).files().unwrap();
        assert_eq!(files.fingerprint(), fingerprint);

        // A path written twice, of other bytes or the same, and a directory that is there, are
        // refused.
        let mut twice = PackageWriter::create(&test_dir.path("three")).unwrap();
        twice.write(path("a.txt"), b"1").unwrap();
        for bytes in [b"2", b"1"] {
            assert!(matches!(
                twice.write(path("a.txt"), bytes),
                Err(WriteError::File(error)) if error.path == test_dir.path("three/a.txt")
            ));
        }
        // The refused writes keep no row and no link's target: a file of the bytes of the
        // refused `2` is a file of its own, and the index lists `a.txt` as its first write.
        twice.write(path("b.txt"), b"2").unwrap();
        assert!(!test_dir.same_file("three/a.txt", "three/b.txt"));
        assert_eq!(test_dir.read("three/b.txt"), b"2");
        twice.finish().unwrap();
        let rows = format!(
            "campfire/package-index/v2\n{} 1 a.txt\n{} 1 b.txt\n",
            hex(b"1"),
            hex(b"2")
        );
        assert_eq!(test_dir.read("three/package.index"), rows.as_bytes());
        assert!(matches!(
            PackageWriter::create(&test_dir.path("one")),
            Err(WriteError::CreateDir(_))
        ));
    }
}
