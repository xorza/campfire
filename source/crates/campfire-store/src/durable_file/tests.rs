use tempfile::TempDir;

use super::*;

#[test]
fn a_durable_write_replaces_a_file_whole_and_leaves_no_temporary_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state");
    DurableFile::write(&path, b"first, and longer").unwrap();
    DurableFile::write(&path, b"second").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"second");
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["state"]);
    assert_eq!(OwnerOnly::exposure_at(&path).unwrap(), None);

    // A crash left a temporary file, which others may read: the write makes its own, so the file
    // is its owner's only.
    let stale = dir.path().join("state.part");
    fs::write(&stale, b"stale and longer").unwrap();
    OwnerOnly::expose(&stale);
    assert!(OwnerOnly::exposure_at(&stale).unwrap().is_some());
    DurableFile::write(&path, b"third").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"third");
    assert!(!stale.exists());
    assert_eq!(OwnerOnly::exposure_at(&path).unwrap(), None);

    // A path with no file name, and a directory that is not there.
    assert!(matches!(
        DurableFile::write(Path::new("/"), b""),
        Err(DurableError::NoName)
    ));
    let lost = dir.path().join("missing").join("state");
    assert!(matches!(
        DurableFile::write(&lost, b""),
        Err(DurableError::Create(_))
    ));
}

#[test]
fn a_durable_directory_is_made_once_and_its_owners_only() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sessions");
    DurableFile::create_dir(&path).unwrap();
    DurableFile::write(&path.join("kept"), b"kept").unwrap();
    // Made again, it stays as it is.
    DurableFile::create_dir(&path).unwrap();
    assert_eq!(fs::read(path.join("kept")).unwrap(), b"kept");
    assert_eq!(OwnerOnly::exposure_at(&path).unwrap(), None);
    // A file in its place is no directory.
    assert!(matches!(
        DurableFile::create_dir(&path.join("kept")),
        Err(DurableError::Create(_))
    ));
    // Removed, it goes with all it holds; a directory that is not there is not removed.
    DurableFile::remove_dir(&path).unwrap();
    assert!(!path.exists());
    assert!(matches!(
        DurableFile::remove_dir(&path),
        Err(DurableError::Remove(_))
    ));
}
