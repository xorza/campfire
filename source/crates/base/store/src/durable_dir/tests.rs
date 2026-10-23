use tempfile::TempDir;

use super::*;
use crate::dir_entries::DirEntries;
use crate::durable_file::DurableFile;
use crate::test_dir::TestDir;

#[test]
fn a_durable_directory_is_made_once_and_its_users_only() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sessions");
    DurableDir::create(&path).unwrap();
    DurableFile::write(&path.join("kept"), b"kept").unwrap();
    // Made again, it stays as it is.
    DurableDir::create(&path).unwrap();
    assert_eq!(fs::read(path.join("kept")).unwrap(), b"kept");
    assert_eq!(UserOnly::exposure_at(&path).unwrap(), None);
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
    // A link to a directory in its name is refused by each make, not followed; a directory above
    // it may be a link.
    let test_dir = TestDir::new();
    test_dir.create_dir("real");
    test_dir.link_dir("real", "link");
    let link = test_dir.path("link");
    for create in [DurableDir::create, DurableDir::create_all] {
        assert!(matches!(
            create(&link),
            Err(PathError {
                error: DurableDirError::Link,
                ..
            })
        ));
    }
    DurableDir::create_all(&link.join("under")).unwrap();
    assert_eq!(
        DirEntries::read(&test_dir.path("real")).unwrap()[0].name,
        "under"
    );
    // Each missing directory above one is made, its user's only; one already there stays. A new
    // directory is made once, and a file holds its name as one does.
    let deep = dir.path().join("a").join("b");
    DurableDir::create_all(&deep).unwrap();
    DurableDir::create_all(&deep).unwrap();
    assert_eq!(UserOnly::exposure_at(&dir.path().join("a")).unwrap(), None);
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
    // Removed, it goes with all it holds; a directory that is not there is not removed. A
    // tombstone a crash left as it removed another directory is in no listing, and goes with
    // the next removal in its parent.
    let left = dir.path().join(".campfire-removed.left");
    DurableDir::create(&left).unwrap();
    DurableFile::write(&left.join("held"), b"held").unwrap();
    let listed = || -> Vec<OsString> {
        let entries = DirEntries::read(dir.path()).unwrap();
        entries.into_iter().map(|entry| entry.name).collect()
    };
    assert_eq!(listed(), ["a", "sessions"]);
    DurableDir::remove(&path).unwrap();
    assert!(!path.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    assert_eq!(listed(), ["a"]);
    assert!(matches!(
        DurableDir::remove(&path),
        Err(PathError {
            error: DurableDirError::Remove(_),
            ..
        })
    ));
}
