use super::*;
use crate::test_dir::TestDir;

#[test]
fn a_secret_file_reads_its_users_bytes_alone_and_says_why_it_refuses() {
    let test_dir = TestDir::new();
    let secret = |name| SecretFile::at(test_dir.path(name));
    let refused = |file: &SecretFile, max_len| file.read(max_len).map(|_| ()).unwrap_err().error;
    let file = secret("player.nsec");
    file.create_new(b"secret\n").unwrap();
    assert!(matches!(
        file.create_new(b"other\n"),
        Err(PathError {
            error: DurableCreateError::Exists,
            ..
        })
    ));
    assert_eq!(*file.read(7).unwrap(), b"secret\n");
    // A read of `max_len` bytes leaves the buffer at the `max_len + 1` it was made with, so it
    // never grew; one byte past `max_len` is too large.
    assert_eq!(file.read(7).unwrap().capacity(), 8);
    assert!(matches!(refused(&file, 6), ReadError::TooLarge { max: 6 }));
    let empty = secret("empty");
    empty.create_new(b"").unwrap();
    assert_eq!(*empty.read(0).unwrap(), b"");
    assert_eq!(test_dir.exposure("player.nsec"), None);

    assert!(matches!(
        refused(&secret("none.nsec"), 64),
        ReadError::Missing
    ));
    test_dir.create_dir("dir");
    assert!(matches!(refused(&secret("dir"), 64), ReadError::NotFile));
    // A name no OS takes fails before any call, the same on each.
    assert!(matches!(
        refused(&secret("bad\0name"), 64),
        ReadError::Read(_)
    ));

    // Others may read it: refused, with who they are, before its bytes are read.
    file.expose();
    let exposure = test_dir.exposure("player.nsec").unwrap();
    assert!(matches!(
        file.read(64),
        Err(PathError { path, error: ReadError::Exposed(refused) })
            if refused == exposure && path == test_dir.path("player.nsec")
    ));
    // Replaced, it is its user's only once more.
    file.replace(b"again\n").unwrap();
    assert_eq!(*file.read(64).unwrap(), b"again\n");
    assert_eq!(
        file.to_string(),
        test_dir.path("player.nsec").display().to_string()
    );
}
