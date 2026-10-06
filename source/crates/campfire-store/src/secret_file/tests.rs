use std::fs;

use super::*;
use crate::durable_file::tests::ScratchDir;
#[cfg(unix)]
use crate::durable_file::tests::mode;

#[test]
fn a_secret_file_round_trips_and_refuses_one_others_may_read() {
    let dir = ScratchDir::new("secret-file");
    let path = dir.0.join("player.nsec");
    SecretFile::write(&path, b"secret\n").unwrap();
    assert_eq!(SecretFile::read(&path).unwrap(), b"secret\n");
    assert!(matches!(
        SecretFile::read(&dir.0.join("none.nsec")),
        Err(SecretReadError::Read(_))
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(mode(&path), 0o600);
        // Others may read it: refused, with its mode, before its bytes are read.
        for mode in [0o644, 0o640, 0o604] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            assert!(matches!(
                SecretFile::read(&path),
                Err(SecretReadError::Exposed { mode: refused }) if refused == mode
            ));
        }
        // Written again, it is its owner's only once more.
        SecretFile::write(&path, b"again\n").unwrap();
        assert_eq!(SecretFile::read(&path).unwrap(), b"again\n");
    }
}
