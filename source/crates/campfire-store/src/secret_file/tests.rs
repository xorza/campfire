use tempfile::TempDir;

use super::*;

#[test]
fn a_secret_file_round_trips_and_refuses_one_others_may_read() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("player.nsec");
    SecretFile::write(&path, b"secret\n").unwrap();
    assert_eq!(SecretFile::read(&path).unwrap(), b"secret\n");
    assert!(matches!(
        SecretFile::read(&dir.path().join("none.nsec")),
        Err(SecretReadError::Read(_))
    ));
    assert_eq!(OwnerOnly::exposure_at(&path).unwrap(), None);
    // Others may read it: refused, with who they are, before its bytes are read.
    SecretFile::expose(&path);
    let exposure = OwnerOnly::exposure_at(&path).unwrap().unwrap();
    assert!(matches!(
        SecretFile::read(&path),
        Err(SecretReadError::Exposed(refused)) if refused == exposure
    ));
    // Written again, it is its owner's only once more.
    SecretFile::write(&path, b"again\n").unwrap();
    assert_eq!(SecretFile::read(&path).unwrap(), b"again\n");
}
