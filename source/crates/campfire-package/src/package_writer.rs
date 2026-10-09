use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use campfire_capabilities::PackagePath;
use campfire_common::Fingerprint;
use campfire_store::{DurableFile, OutputFile};

use crate::error::WriteError;
use crate::file_index::{FileIndex, FileRow};

/// A new package as its builder writes it, an importer or a packer: each file into a new directory, written once with no sync
/// and its row kept, then the index of the rows, synced, so every file is checked against what
/// was written whenever it is read, and a write a crash cut short is found then.
#[derive(Debug)]
pub struct PackageWriter {
    root: PathBuf,
    rows: BTreeMap<PackagePath, FileRow>,
}

impl PackageWriter {
    /// A writer of the package in the new directory `root`; an error when one is there.
    pub fn create(root: &Path) -> Result<PackageWriter, WriteError> {
        DurableFile::create_new_dir(root).map_err(WriteError::CreateDir)?;
        Ok(PackageWriter {
            root: root.to_owned(),
            rows: BTreeMap::new(),
        })
    }

    /// Writes the file at `path` with `bytes`, and the directories on its way; an error for a
    /// path written before.
    pub fn write(&mut self, path: PackagePath, bytes: &[u8]) -> Result<(), WriteError> {
        let at = path
            .as_str()
            .split('/')
            .fold(self.root.clone(), |at, name| at.join(name));
        if let Some(dir) = at.parent() {
            DurableFile::create_dir_all(dir).map_err(WriteError::Directory)?;
        }
        OutputFile::write_new(&at, bytes).map_err(WriteError::File)?;
        self.rows.insert(path, FileRow::of(bytes));
        Ok(())
    }

    /// Writes the package's index; its fingerprint.
    pub fn finish(self) -> Result<Fingerprint, WriteError> {
        let index = FileIndex::new(self.rows).map_err(WriteError::Package)?;
        DurableFile::write(&self.root.join(FileIndex::PATH), index.bytes())
            .map_err(WriteError::Index)?;
        Ok(index.fingerprint())
    }
}

#[cfg(test)]
mod tests {
    use campfire_store::Scratch;
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
        // The tag, then the rows in path order: `a.txt` before `data/b.toml`.
        let index = [
            &b"campfire/package-index/v1"[..],
            &[2, 5],
            b"a.txt",
            &[2],
            &Sha256::digest(b"aa"),
            &[11],
            b"data/b.toml",
            &[1],
            &Sha256::digest(b"b"),
        ]
        .concat();
        assert_eq!(scratch.read("one/package.index"), index);
        assert_eq!(fingerprint, Fingerprint::new(Sha256::digest(&index).into()));
        assert_eq!(
            PackageDir::new(scratch.path("one"))
                .read()
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

        // A path written twice, and a directory that is there, are refused.
        let mut twice = PackageWriter::create(&scratch.path("three")).unwrap();
        twice.write(path("a.txt"), b"1").unwrap();
        assert!(matches!(
            twice.write(path("a.txt"), b"2"),
            Err(WriteError::File(error)) if error.path == scratch.path("three/a.txt")
        ));
        assert!(matches!(
            PackageWriter::create(&scratch.path("one")),
            Err(WriteError::CreateDir(_))
        ));
    }
}
