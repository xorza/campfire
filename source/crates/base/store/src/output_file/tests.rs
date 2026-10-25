use super::*;
use crate::input_file::InputFile;
use crate::test_dir::TestDir;

#[test]
fn a_new_file_holds_its_bytes_and_never_replaces_one() {
    let test_dir = TestDir::new();
    let path = test_dir.path("one");
    OutputFile::write_new(&path, b"abc").unwrap();
    assert_eq!(InputFile::read(&path, 3).unwrap(), b"abc");
    assert_eq!(UserOnly::exposure_at(&path).unwrap(), None);
    let error = OutputFile::write_new(&path, b"x").unwrap_err();
    assert_eq!(error.error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(error.path, path);
    assert_eq!(InputFile::read(&path, 3).unwrap(), b"abc");
    // A missing directory on its way is the error, with the path.
    let deep = test_dir.path("no/such");
    assert_eq!(
        OutputFile::write_new(&deep, b"").unwrap_err().error.kind(),
        io::ErrorKind::NotFound
    );
}
