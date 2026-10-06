use std::fs;

use super::*;
use crate::durable_file::tests::ScratchDir;
#[cfg(unix)]
use crate::durable_file::tests::mode;

#[test]
fn a_data_directory_is_made_its_owners_only_and_holds_one_holder() {
    let scratch = ScratchDir::new("data-dir");
    let path = scratch.0.join("nested").join("data");
    let first = DataDir::open(&path).unwrap();
    assert_eq!(first.path(), path);
    #[cfg(unix)]
    assert_eq!((mode(&path), mode(&path.join("lock"))), (0o700, 0o600));
    // A second holder while the first holds it is refused; once it drops, it takes it.
    assert!(matches!(DataDir::open(&path), Err(DataDirError::Locked)));
    drop(first);
    let again = DataDir::open(&path).unwrap();
    assert!(matches!(DataDir::open(&path), Err(DataDirError::Locked)));
    drop(again);
    // A file in its place is no directory.
    let file = scratch.0.join("file");
    fs::write(&file, b"").unwrap();
    assert!(matches!(DataDir::open(&file), Err(DataDirError::Create(_))));
}
