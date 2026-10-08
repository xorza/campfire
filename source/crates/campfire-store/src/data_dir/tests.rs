use std::fs;

use tempfile::TempDir;

use super::*;

#[test]
fn a_data_directory_is_made_its_owners_only_and_holds_one_holder() {
    let scratch = TempDir::new().unwrap();
    let path = scratch.path().join("nested").join("data");
    let first = DataDir::open(&path).unwrap();
    assert_eq!(first.path(), path);
    // Each directory it made and its lock are their owner's only.
    for made in [
        scratch.path().join("nested"),
        path.clone(),
        path.join("lock"),
    ] {
        assert_eq!(
            OwnerOnly::exposure_at(&made).unwrap(),
            None,
            "{}",
            made.display()
        );
    }
    // A second holder while the first holds it is refused; once it drops, it takes it.
    assert!(matches!(DataDir::open(&path), Err(DataDirError::Locked)));
    drop(first);
    let again = DataDir::open(&path).unwrap();
    assert!(matches!(DataDir::open(&path), Err(DataDirError::Locked)));
    drop(again);
    // A file in its place is no directory.
    let file = scratch.path().join("file");
    fs::write(&file, b"").unwrap();
    assert!(matches!(DataDir::open(&file), Err(DataDirError::Create(_))));
}
