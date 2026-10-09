use super::*;
use crate::scratch::Scratch;

#[test]
fn a_secret_file_reads_its_owners_bytes_alone_and_says_why_it_refuses() {
    let scratch = Scratch::new();
    let secret = |name| SecretFile::at(scratch.path(name));
    let refused = |file: &SecretFile, max_len| file.read(max_len).map(|_| ()).unwrap_err().error;
    let file = secret("player.nsec");
    file.create(b"secret\n").unwrap();
    assert!(matches!(
        file.create(b"other\n"),
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
    empty.create(b"").unwrap();
    assert_eq!(*empty.read(0).unwrap(), b"");
    assert_eq!(scratch.exposure("player.nsec"), None);

    assert!(matches!(
        refused(&secret("none.nsec"), 64),
        ReadError::Missing
    ));
    scratch.create_dir("dir");
    assert!(matches!(refused(&secret("dir"), 64), ReadError::NotFile));
    // A name no OS takes fails before any call, the same on each.
    assert!(matches!(
        refused(&secret("bad\0name"), 64),
        ReadError::Read(_)
    ));

    // Others may read it: refused, with who they are, before its bytes are read.
    file.expose();
    let exposure = scratch.exposure("player.nsec").unwrap();
    assert!(matches!(
        file.read(64),
        Err(PathError { path, error: ReadError::Exposed(refused) })
            if refused == exposure && path == scratch.path("player.nsec")
    ));
    // Replaced, it is its owner's only once more.
    file.replace(b"again\n").unwrap();
    assert_eq!(*file.read(64).unwrap(), b"again\n");
    assert_eq!(
        file.to_string(),
        scratch.path("player.nsec").display().to_string()
    );
}
