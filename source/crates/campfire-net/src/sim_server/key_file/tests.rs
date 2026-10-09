use std::sync::Barrier;
use std::{fs, thread};

use tempfile::TempDir;

use super::*;

#[test]
fn a_key_file_round_trips_and_refuses_what_is_no_private_nsec() {
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path();
    let path = dir.join("player.nsec");
    let file = SecretFile::at(path.clone());
    // Made when missing, from the random bytes given, then read back the same.
    let made = KeyFile::read_or_create(&file, |bytes| bytes.fill(7)).unwrap();
    assert_eq!(made.secret_bytes(), [7; 32]);
    assert_eq!(KeyFile::read(&file).unwrap(), made);
    assert_eq!(
        KeyFile::read_or_create(&file, |bytes| bytes.fill(9)).unwrap(),
        made
    );
    assert_eq!(fs::read(&path).unwrap(), Nsec::encode(&made).as_bytes());

    // Text that is no key; a file that is not there; one longer than a key file may be.
    let bad = SecretFile::at(dir.join("bad.nsec"));
    bad.create(b"secret").unwrap();
    assert!(matches!(KeyFile::read(&bad), Err(KeyFileError::NotNsec(_))));
    assert!(matches!(
        KeyFile::read(&SecretFile::at(dir.join("none.nsec"))),
        Err(KeyFileError::Read(SecretReadError::Missing))
    ));
    let long = SecretFile::at(dir.join("long.nsec"));
    long.create(&[b' '; Nsec::MAX_FILE_LEN + 1]).unwrap();
    assert!(matches!(
        KeyFile::read(&long),
        Err(KeyFileError::Read(SecretReadError::TooLarge { .. }))
    ));
    // Others may read it: refused before its key is read, so text that is no key is refused as
    // exposed too, and a file of such a key is no missing one, which a new key would replace.
    for file in [&file, &bad] {
        file.expose();
        assert!(matches!(
            KeyFile::read(file),
            Err(KeyFileError::Read(SecretReadError::Exposed(_)))
        ));
    }
    assert!(matches!(
        KeyFile::read_or_create(&file, |bytes| bytes.fill(9)),
        Err(KeyFileError::Read(SecretReadError::Exposed(_)))
    ));
}

#[test]
fn two_writers_that_make_one_key_file_at_once_share_one_key() {
    // Each draws its own key, of 1s or 2s; one creates the file, and the other reads its key.
    let scratch = TempDir::new().unwrap();
    let file = SecretFile::at(scratch.path().join("player.nsec"));
    let barrier = Barrier::new(2);
    #[expect(
        clippy::disallowed_methods,
        reason = "a test races two writers of one key file on threads of its own"
    )]
    let keys = thread::scope(|scope| {
        let fills: [fn(&mut [u8; 32]); 2] = [|bytes| bytes.fill(1), |bytes| bytes.fill(2)];
        let threads = fills.map(|fill| {
            let (file, barrier) = (&file, &barrier);
            scope.spawn(move || {
                barrier.wait();
                KeyFile::read_or_create(file, fill).unwrap()
            })
        });
        threads.map(|thread| thread.join().unwrap())
    });
    assert_eq!(keys[0], keys[1]);
    assert!([[1; 32], [2; 32]].contains(&keys[0].secret_bytes()));
    assert_eq!(KeyFile::read(&file).unwrap(), keys[0]);
}
