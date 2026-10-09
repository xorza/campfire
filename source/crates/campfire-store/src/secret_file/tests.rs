use tempfile::TempDir;

use super::*;

#[test]
fn a_secret_file_reads_its_owners_bytes_alone_and_says_why_it_refuses() {
    let dir = TempDir::new().unwrap();
    let secret = |name| SecretFile::at(dir.path().join(name));
    let file = secret("player.nsec");
    file.create(b"secret\n").unwrap();
    assert!(matches!(
        file.create(b"other\n"),
        Err(DurableCreateError::Exists)
    ));
    assert_eq!(*file.read(7).unwrap(), b"secret\n");
    // A read of `max_len` bytes leaves the buffer at the `max_len + 1` it was made with, so it
    // never grew; one byte past `max_len` is too large.
    assert_eq!(file.read(7).unwrap().capacity(), 8);
    assert!(matches!(
        file.read(6),
        Err(SecretReadError::TooLarge { max: 6 })
    ));
    let empty = secret("empty");
    empty.create(b"").unwrap();
    assert_eq!(*empty.read(0).unwrap(), b"");
    assert_eq!(
        OwnerOnly::exposure_at(&dir.path().join("player.nsec")).unwrap(),
        None
    );

    assert!(matches!(
        secret("none.nsec").read(64),
        Err(SecretReadError::Missing)
    ));
    fs::create_dir(dir.path().join("dir")).unwrap();
    assert!(matches!(
        secret("dir").read(64),
        Err(SecretReadError::NotFile)
    ));
    // A name no OS takes fails before any call, the same on each.
    assert!(matches!(
        secret("bad\0name").read(64),
        Err(SecretReadError::Read(_))
    ));

    // Others may read it: refused, with who they are, before its bytes are read.
    file.expose();
    let exposure = OwnerOnly::exposure_at(&dir.path().join("player.nsec"))
        .unwrap()
        .unwrap();
    assert!(matches!(
        file.read(64),
        Err(SecretReadError::Exposed(refused)) if refused == exposure
    ));
    // Replaced, it is its owner's only once more.
    file.replace(b"again\n").unwrap();
    assert_eq!(*file.read(64).unwrap(), b"again\n");
    assert_eq!(
        file.to_string(),
        dir.path().join("player.nsec").display().to_string()
    );
}
