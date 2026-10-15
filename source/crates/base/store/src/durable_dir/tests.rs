use tempfile::TempDir;

use super::*;
use crate::durable_file::DurableFile;

#[test]
fn a_durable_directory_is_made_once_and_its_owners_only() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sessions");
    DurableDir::create(&path).unwrap();
    DurableFile::write(&path.join("kept"), b"kept").unwrap();
    // Made again, it stays as it is.
    DurableDir::create(&path).unwrap();
    assert_eq!(fs::read(path.join("kept")).unwrap(), b"kept");
    assert_eq!(OwnerOnly::exposure_at(&path).unwrap(), None);
    // A file in its place is no directory; a path with no parent names none.
    assert!(matches!(
        DurableDir::create(&path.join("kept")),
        Err(PathError {
            error: DurableDirError::Create(_),
            ..
        })
    ));
    assert!(matches!(
        DurableDir::create(Path::new("/")),
        Err(PathError {
            error: DurableDirError::NoParent,
            ..
        })
    ));
    // Each missing directory above one is made, its owner's only; one already there stays. A new
    // directory is made once, and a file holds its name as one does.
    let deep = dir.path().join("a").join("b");
    DurableDir::create_all(&deep).unwrap();
    DurableDir::create_all(&deep).unwrap();
    assert_eq!(OwnerOnly::exposure_at(&dir.path().join("a")).unwrap(), None);
    DurableDir::create_new(&deep.join("run")).unwrap();
    for taken in [deep.join("run"), path.join("kept")] {
        assert!(matches!(
            DurableDir::create_new(&taken),
            Err(PathError {
                error: DurableDirCreateError::Exists,
                ..
            })
        ));
    }
    // Renamed, it takes the new name with all it holds; one that is not there is not renamed.
    let renamed = dir.path().join("renamed");
    DurableDir::rename(&path, &renamed).unwrap();
    assert!(!path.exists());
    assert_eq!(fs::read(renamed.join("kept")).unwrap(), b"kept");
    assert!(matches!(
        DurableDir::rename(&path, &renamed),
        Err(PathError {
            error: DurableDirError::Rename(_),
            ..
        })
    ));
    DurableDir::rename(&renamed, &path).unwrap();
    // Removed, it goes with all it holds; a directory that is not there is not removed.
    DurableDir::remove(&path).unwrap();
    assert!(!path.exists());
    assert!(matches!(
        DurableDir::remove(&path),
        Err(PathError {
            error: DurableDirError::Remove(_),
            ..
        })
    ));
}
