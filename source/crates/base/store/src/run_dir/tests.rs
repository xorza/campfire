use std::time::Duration;

use super::*;
use crate::durable_dir::error::DurableDirCreateError;
use crate::path_error::PathError;
use crate::test_dir::TestDir;

fn at(millis: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(millis)
}

#[test]
fn each_run_gets_a_directory_of_its_own_named_for_its_start() {
    assert_eq!(RunDir::name(at(0)), "1970-01-01T00-00-00.000Z");
    // 1 790 000 000 s is 20 717 days and 51 200 s: 14 h 13 min 20 s into 2026-09-21. 2026
    // begins 20 454 days after 1970, so the day is its 264th.
    assert_eq!(
        RunDir::name(at(1_790_000_000_123)),
        "2026-09-21T14-13-20.123Z"
    );
    // The leap day of 2000, 11 016 days after 1970, and the last millisecond of that day.
    assert_eq!(
        RunDir::name(at(11_016 * 86_400_000 + 86_399_999)),
        "2000-02-29T23-59-59.999Z"
    );

    // Below a directory that goes when the test ends, passed or failed.
    let test_dir = TestDir::new();
    let root = test_dir.path("runs");
    let three = NonZeroUsize::new(3).unwrap();
    let first = RunDir::create(&root, at(1_000), three).unwrap();
    let kept = Path::new("runs")
        .join(RunDir::name(at(1_000)))
        .join("server.jsonl");
    test_dir.write(&kept, "kept");
    let second = RunDir::create(&root, at(2_000), three).unwrap();
    assert_ne!(first.path(), second.path());
    assert_eq!(test_dir.read_text(&kept), "kept");
    assert!(matches!(
        RunDir::create(&root, at(1_000), three),
        Err(RunDirError::Dir(PathError {
            error: DurableDirCreateError::Exists,
            ..
        }))
    ));

    // Kept 3, the fourth run removes the first, its file with it, and the fifth the second;
    // a file, and a directory of another name, stay.
    test_dir.write("runs/notes.txt", "notes");
    test_dir.create_dir("runs/1970-01-01T00-00-00.000");
    RunDir::create(&root, at(3_000), three).unwrap();
    RunDir::create(&root, at(4_000), three).unwrap();
    let names = |test_dir: &TestDir| test_dir.names("runs");
    assert_eq!(
        names(&test_dir),
        [
            "1970-01-01T00-00-00.000",
            "1970-01-01T00-00-02.000Z",
            "1970-01-01T00-00-03.000Z",
            "1970-01-01T00-00-04.000Z",
            "notes.txt",
        ]
    );
    RunDir::create(&root, at(5_000), three).unwrap();
    assert_eq!(
        names(&test_dir)[1..4],
        [
            "1970-01-01T00-00-03.000Z",
            "1970-01-01T00-00-04.000Z",
            "1970-01-01T00-00-05.000Z",
        ]
    );
    // One kept is the run's own alone.
    RunDir::create(&root, at(6_000), NonZeroUsize::MIN).unwrap();
    assert_eq!(
        names(&test_dir),
        [
            "1970-01-01T00-00-00.000",
            "1970-01-01T00-00-06.000Z",
            "notes.txt",
        ]
    );
    // A run that starts before the others, as after the clock went back, keeps its own.
    RunDir::create(&root, at(500), NonZeroUsize::MIN).unwrap();
    assert_eq!(
        names(&test_dir),
        [
            "1970-01-01T00-00-00.000",
            "1970-01-01T00-00-00.500Z",
            "notes.txt",
        ]
    );
}
