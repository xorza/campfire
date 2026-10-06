use std::fs;

use tempfile::TempDir;

use super::*;

#[test]
fn a_key_file_round_trips_and_refuses_what_is_no_private_nsec() {
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path();
    let path = dir.join("player.nsec");
    // Made when missing, from the random bytes given, then read back the same.
    let made = KeyFile::read_or_create(&path, |bytes| bytes.fill(7)).unwrap();
    assert_eq!(made.secret_bytes(), [7; 32]);
    assert_eq!(KeyFile::read(&path).unwrap(), made);
    assert_eq!(
        KeyFile::read_or_create(&path, |bytes| bytes.fill(9)).unwrap(),
        made
    );
    assert_eq!(fs::read(&path).unwrap(), Nsec::encode(&made).as_bytes());

    // Text that is no key; a file that is not there.
    let bad = dir.join("bad.nsec");
    SecretFile::write(&bad, b"secret").unwrap();
    assert!(matches!(KeyFile::read(&bad), Err(KeyFileError::NotNsec(_))));
    assert!(matches!(
        KeyFile::read(&dir.join("none.nsec")),
        Err(KeyFileError::Read(SecretReadError::Read(_)))
    ));
    // Others may read it: refused before its key is read, so text that is no key is refused as
    // exposed too, and a file of such a key is no missing one, which a new key would replace.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&path, &bad] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
            assert!(matches!(
                KeyFile::read(path),
                Err(KeyFileError::Read(SecretReadError::Exposed { mode: 0o640 }))
            ));
        }
        assert!(matches!(
            KeyFile::read_or_create(&path, |bytes| bytes.fill(9)),
            Err(KeyFileError::Read(SecretReadError::Exposed { mode: 0o640 }))
        ));
    }
}
