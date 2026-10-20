use tempfile::TempDir;

use super::*;
use campfire_threads::internals::Race;

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
    assert_eq!(UserOnly::exposure_at(&path).unwrap(), None);

    // A crash left a temporary file, which others may read: the write makes its own, so the file
    // is its user's only.
    let stale = dir.path().join("state.part");
    fs::write(&stale, b"stale and longer").unwrap();
    UserOnly::expose(&stale);
    assert!(UserOnly::exposure_at(&stale).unwrap().is_some());
    DurableFile::write(&path, b"third").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"third");
    assert!(!stale.exists());
    assert_eq!(UserOnly::exposure_at(&path).unwrap(), None);

    // A path with no file name, and a directory that is not there.
    assert!(matches!(
        DurableFile::write(Path::new("/"), b""),
        Err(PathError {
            error: DurableError::NoName,
            ..
        })
    ));
    // The error names the path it concerns.
    let lost = dir.path().join("missing").join("state");
    assert!(matches!(
        DurableFile::write(&lost, b""),
        Err(PathError { path, error: DurableError::Create(_) }) if path == lost
    ));
}

#[test]
fn a_durable_create_makes_a_file_once_and_never_replaces_it() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("server.nsec");
    DurableFile::create_new(&path, b"first").unwrap();
    assert!(matches!(
        DurableFile::create_new(&path, b"second"),
        Err(PathError {
            error: DurableCreateError::Exists,
            ..
        })
    ));
    assert_eq!(fs::read(&path).unwrap(), b"first");
    assert_eq!(UserOnly::exposure_at(&path).unwrap(), None);

    // Two threads make one file at once: one makes it, the other finds it made, and the file
    // holds the maker's bytes, with no temporary file left.
    let raced = dir.path().join("raced.nsec");
    let results = Race::run(2, |index| {
        let bytes = [&b"one"[..], b"two"][index];
        (bytes, DurableFile::create_new(&raced, bytes))
    });
    let made: Vec<_> = results
        .iter()
        .filter(|(_, result)| result.is_ok())
        .map(|(bytes, _)| *bytes)
        .collect();
    assert_eq!(made.len(), 1, "{results:?}");
    assert!(results.iter().any(|(_, result)| matches!(
        result,
        Err(PathError {
            error: DurableCreateError::Exists,
            ..
        })
    )));
    assert_eq!(fs::read(&raced).unwrap(), made[0]);
    let mut names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "raced.nsec",
            "raced.nsec.lock",
            "server.nsec",
            "server.nsec.lock"
        ]
    );

    // A crash left the temporary of a create, a copy of a secret: the next create of its name
    // removes it before it writes its own, and leaves none.
    let crashed = dir.path().join("crashed.nsec");
    fs::write(dir.path().join("crashed.nsec.new"), b"lost").unwrap();
    DurableFile::create_new(&crashed, b"made").unwrap();
    assert_eq!(fs::read(&crashed).unwrap(), b"made");
    assert!(!dir.path().join("crashed.nsec.new").exists());
}
