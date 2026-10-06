use std::fs;

use super::*;
use crate::durable_file::tests::ScratchDir;

#[test]
fn a_key_file_round_trips_and_refuses_what_is_no_private_nsec() {
    let dir = ScratchDir::new("key-file");
    let path = dir.0.join("player.nsec");
    // Made when missing, from the random bytes given, then read back the same.
    let made = KeyFile::read_or_create(&path, |bytes| bytes.fill(7)).unwrap();
    assert_eq!(made.secret_bytes(), [7; 32]);
    assert_eq!(KeyFile::read(&path).unwrap(), made);
    assert_eq!(
        KeyFile::read_or_create(&path, |bytes| bytes.fill(9)).unwrap(),
        made
    );
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("nsec1") && text.ends_with('\n'), "{text}");

    // An npub, and text that is no key.
    let bad = dir.0.join("bad.nsec");
    for wrong in [
        "npub1sg6plzptd64u62a878hep2kev88swjh3tw00gjsfl8f237lmu63q0uf63m",
        "secret",
    ] {
        DurableFile::write(&bad, wrong.as_bytes()).unwrap();
        assert!(matches!(KeyFile::read(&bad), Err(KeyFileError::NotNsec(_))));
    }
    assert!(matches!(
        KeyFile::read(&dir.0.join("none.nsec")),
        Err(KeyFileError::Read(_))
    ));
    // Others may read it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            KeyFile::read(&path),
            Err(KeyFileError::Exposed { mode: 0o644 })
        ));
        // The mode is checked before the key is read: text that is no key, which others may
        // read, is refused as exposed.
        fs::set_permissions(&bad, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(matches!(
            KeyFile::read(&bad),
            Err(KeyFileError::Exposed { mode: 0o640 })
        ));
    }
}
