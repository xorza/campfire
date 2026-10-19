use super::*;
use crate::data_dir::DataDir;

#[test]
fn a_test_directory_holds_a_tests_files_its_users_only() {
    let test_dir = TestDir::new();
    assert_eq!(test_dir.exposure("."), None);
    // A file in a directory that is missing makes it; it reads back, lists and goes.
    test_dir.write("nested/file", "text");
    assert_eq!(test_dir.read("nested/file"), b"text");
    assert_eq!(test_dir.read_text("nested/file"), "text");
    assert_eq!(test_dir.names("nested"), ["file"]);
    assert!(test_dir.exists("nested/file"));
    assert_eq!(test_dir.kind("nested/file"), Some(EntryKind::File));
    assert_eq!(test_dir.kind("nested"), Some(EntryKind::Dir));
    assert_eq!(test_dir.kind("none"), None);
    // An absolute path under its root is within it.
    assert_eq!(test_dir.read(test_dir.path("nested/file")), b"text");
    test_dir.expose("nested/file");
    assert!(test_dir.exposure("nested/file").is_some());
    test_dir.rename("nested/file", "nested/moved");
    assert_eq!(test_dir.names("nested"), ["moved"]);
    test_dir.remove("nested");
    assert!(!test_dir.exists("nested"));
    assert_eq!(test_dir.root(), test_dir.path("").as_path());
    // A data directory opens in it, as its root lets no one else in.
    drop(DataDir::open(&test_dir.path("data")).unwrap());
    // Its directory goes with it.
    let root = test_dir.path(".");
    drop(test_dir);
    assert!(!root.exists());
}

#[test]
#[should_panic(expected = "leaves the test directory")]
fn a_path_up_from_the_test_directory_is_refused() {
    TestDir::new().path("../out");
}

#[test]
#[should_panic(expected = "leaves the test directory")]
fn an_absolute_path_outside_it_is_refused() {
    let (test_dir, other) = (TestDir::new(), TestDir::new());
    test_dir.path(other.path("file"));
}
