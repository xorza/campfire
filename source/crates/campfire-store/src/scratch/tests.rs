use super::*;
use crate::data_dir::DataDir;

#[test]
fn a_scratch_directory_holds_a_tests_files_its_owners_only() {
    let scratch = Scratch::new();
    assert_eq!(scratch.exposure("."), None);
    // A file in a directory that is missing makes it; it reads back, lists and goes.
    scratch.write("nested/file", "text");
    assert_eq!(scratch.read("nested/file"), b"text");
    assert_eq!(scratch.read_text("nested/file"), "text");
    assert_eq!(scratch.names("nested"), ["file"]);
    assert!(scratch.exists("nested/file"));
    scratch.expose("nested/file");
    assert!(scratch.exposure("nested/file").is_some());
    scratch.remove("nested");
    assert!(!scratch.exists("nested"));
    // A data directory opens in it, as its root lets no one else in.
    drop(DataDir::open(&scratch.path("data")).unwrap());
    // Its directory goes with it.
    let root = scratch.path(".");
    drop(scratch);
    assert!(!root.exists());
}

#[test]
#[should_panic(expected = "leaves the scratch directory")]
fn a_path_up_from_the_scratch_directory_is_refused() {
    Scratch::new().path("../out");
}

#[test]
#[should_panic(expected = "leaves the scratch directory")]
fn an_absolute_path_is_refused() {
    let scratch = Scratch::new();
    let absolute = scratch.path("file");
    scratch.path(absolute);
}
