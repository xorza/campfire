use std::fs;

use crate::test_dir::TestDir;

use super::*;

#[test]
fn a_data_directory_is_made_its_users_only_and_holds_one_holder() {
    let scratch = TestDir::new();
    let path = scratch.root().join("nested").join("data");
    let first = DataDir::open(&path).unwrap();
    assert_eq!(first.path(), path);
    // Each directory it made and its lock are their user's only.
    for made in [
        scratch.root().join("nested"),
        path.clone(),
        path.join("lock"),
    ] {
        assert_eq!(
            UserOnly::exposure_at(&made).unwrap(),
            None,
            "{}",
            made.display()
        );
    }
    // A second holder while the first holds it is refused; once it drops, it takes it.
    assert!(matches!(
        DataDir::open(&path),
        Err(DataDirError::Locked { .. })
    ));
    drop(first);
    let again = DataDir::open(&path).unwrap();
    assert!(matches!(
        DataDir::open(&path),
        Err(DataDirError::Locked { .. })
    ));
    drop(again);
    // A file in its place is no directory.
    let file = scratch.root().join("file");
    fs::write(&file, b"").unwrap();
    assert!(matches!(DataDir::open(&file), Err(DataDirError::Create(_))));
    // A directory others may open is refused, with who they are, before its lock is made.
    let exposed = scratch.root().join("exposed");
    fs::create_dir(&exposed).unwrap();
    UserOnly::expose(&exposed);
    let exposure = UserOnly::exposure_at(&exposed).unwrap().unwrap();
    assert!(matches!(
        DataDir::open(&exposed),
        Err(DataDirError::Exposed { exposure: refused, .. }) if refused == exposure
    ));
    assert_eq!(fs::read_dir(&exposed).unwrap().count(), 0);
}
