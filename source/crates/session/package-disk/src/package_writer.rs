use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::{Path, PathBuf};

use campfire_common::{ContentHash, Fingerprint};
use campfire_package::{FileIndex, PackageWalk};
use campfire_store::{DurableDir, DurableFile, OutputFile};
use campfire_values::PackagePath;

use crate::error::WriteError;

/// A new package as its builder writes it, an importer or a packer: each file into a new
/// directory, written once with no sync and its row kept, then the index of the rows, synced, so
/// every file is checked against what was written whenever it is read, and a write a crash cut
/// short is found then. A file whose SHA-256 an earlier file has is a hard link to that file, so
/// the package keeps one copy of identical files on disk (design 05, Packages).
#[derive(Debug)]
pub struct PackageWriter {
    root: PathBuf,
    rows: PackageWalk,
    /// The first file written of each SHA-256.
    firsts: BTreeMap<ContentHash, PathBuf>,
}

impl PackageWriter {
    /// A writer of the package in the new directory `root`; an error when one is there.
    pub fn create(root: &Path) -> Result<PackageWriter, WriteError> {
        DurableDir::create_new(root).map_err(WriteError::CreateDir)?;
        Ok(PackageWriter {
            root: root.to_owned(),
            rows: PackageWalk::new(),
            firsts: BTreeMap::new(),
        })
    }

    /// Writes the file at `path` with `bytes`, and the directories on its way, linked to the
    /// first file of the same bytes; an error for a path written before.
    pub fn write(&mut self, path: PackagePath, bytes: &[u8]) -> Result<(), WriteError> {
        let at = path
            .as_str()
            .split('/')
            .fold(self.root.clone(), |at, name| at.join(name));
        if let Some(dir) = at.parent() {
            DurableDir::create_all(dir).map_err(WriteError::Directory)?;
        }
        let sha256 = self.rows.add(path, bytes).map_err(WriteError::Package)?;
        let written = match self.firsts.entry(sha256) {
            Entry::Occupied(first) => OutputFile::link_new(&at, first.get(), bytes),
            Entry::Vacant(first) => OutputFile::write_new(first.insert(at), bytes),
        };
        written.map_err(WriteError::File)
    }

    /// Writes the package's index; its fingerprint.
    pub fn finish(self) -> Result<Fingerprint, WriteError> {
        let index = self.rows.finish().map_err(WriteError::Package)?;
        DurableFile::write(&self.root.join(FileIndex::PATH), index.bytes())
            .map_err(WriteError::Index)?;
        Ok(index.fingerprint())
    }
}

#[cfg(test)]
mod tests {
    use campfire_store::internals::Scratch;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::package_dir::PackageDir;

    fn path(text: &str) -> PackagePath {
        PackagePath::parse(text).unwrap()
    }

    #[test]
    fn a_writer_writes_each_file_and_their_index_in_path_order() {
        let scratch = Scratch::new();
        let write = |dir: &str, order: [(&str, &[u8]); 2]| {
            let mut writer = PackageWriter::create(&scratch.path(dir)).unwrap();
            for (at, bytes) in order {
                writer.write(path(at), bytes).unwrap();
            }
            writer.finish().unwrap()
        };
        let fingerprint = write("one", [("data/b.toml", b"b"), ("a.txt", b"aa")]);
        assert_eq!(scratch.read("one/data/b.toml"), b"b");
        assert_eq!(scratch.read("one/a.txt"), b"aa");
        // The tag's line, then the rows in path order: `a.txt` before `data/b.toml`.
        let hex = ContentHash::of;
        let index = format!(
            "campfire/package-index/v2\n{} 2 a.txt\n{} 1 data/b.toml\n",
            hex(b"aa"),
            hex(b"b")
        )
        .into_bytes();
        assert_eq!(scratch.read("one/package.index"), index);
        assert_eq!(fingerprint, Fingerprint::new(Sha256::digest(&index).into()));
        assert_eq!(
            PackageDir::new(scratch.path("one"))
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
        assert_eq!(scratch.read("two/package.index"), index);

        // A file of the bytes of one written before is a link to that file, the first of them in
        // the order of writing: one file on disk, which each path reads whole; the index lists
        // each path, as a writer of copies would.
        let mut linked = PackageWriter::create(&scratch.path("four")).unwrap();
        for (at, bytes) in [
            ("data/b.toml", &b"b"[..]),
            ("a.txt", b"aa"),
            ("c/d.txt", b"aa"),
        ] {
            linked.write(path(at), bytes).unwrap();
        }
        let fingerprint = linked.finish().unwrap();
        assert_eq!(scratch.read("four/c/d.txt"), b"aa");
        assert!(scratch.same_file("four/a.txt", "four/c/d.txt"));
        assert!(!scratch.same_file("four/a.txt", "four/data/b.toml"));
        let rows = format!(
            "campfire/package-index/v2\n{aa} 2 a.txt\n{aa} 2 c/d.txt\n{} 1 data/b.toml\n",
            hex(b"b"),
            aa = hex(b"aa")
        );
        assert_eq!(scratch.read("four/package.index"), rows.as_bytes());
        let files = PackageDir::new(scratch.path("four")).files().unwrap();
        assert_eq!(files.fingerprint(), fingerprint);

        // A path written twice, of other bytes or the same, and a directory that is there, are
        // refused.
        let mut twice = PackageWriter::create(&scratch.path("three")).unwrap();
        twice.write(path("a.txt"), b"1").unwrap();
        for bytes in [b"2", b"1"] {
            assert!(matches!(
                twice.write(path("a.txt"), bytes),
                Err(WriteError::File(error)) if error.path == scratch.path("three/a.txt")
            ));
        }
        assert!(matches!(
            PackageWriter::create(&scratch.path("one")),
            Err(WriteError::CreateDir(_))
        ));
    }
}
