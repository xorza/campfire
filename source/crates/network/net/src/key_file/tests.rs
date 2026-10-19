use campfire_store::internals::{Race, TestDir};

use super::*;

#[test]
fn a_key_file_round_trips_and_refuses_what_is_no_private_nsec() {
    let test_dir = TestDir::new();
    let file = SecretFile::at(test_dir.path("player.nsec"));
    // Made when missing, from the random bytes given, then read back the same.
    let made = KeyFile::read_or_create(&file, Entropy::Fixed(7)).unwrap();
    assert_eq!(*made.secret_bytes(), [7; 32]);
    let key = |signer: Signer| *signer.secret_bytes();
    assert_eq!(
        key(KeyFile::read(&file, Entropy::Fixed(9)).unwrap()),
        [7; 32]
    );
    assert_eq!(
        key(KeyFile::read_or_create(&file, Entropy::Fixed(9)).unwrap()),
        [7; 32]
    );
    assert_eq!(test_dir.read("player.nsec"), encode_nsec(&made).as_bytes());

    // Text that is no key; a file that is not there; one longer than a key file may be.
    let bad = SecretFile::at(test_dir.path("bad.nsec"));
    bad.create_new(b"secret").unwrap();
    assert!(matches!(
        KeyFile::read(&bad, Entropy::Fixed(0)),
        Err(KeyFileError::NotNsec(_))
    ));
    assert!(matches!(
        KeyFile::read(
            &SecretFile::at(test_dir.path("none.nsec")),
            Entropy::Fixed(0)
        ),
        Err(KeyFileError::Read(PathError {
            error: ReadError::Missing,
            ..
        }))
    ));
    let long = SecretFile::at(test_dir.path("long.nsec"));
    long.create_new(&[b' '; MAX_NSEC_FILE_LEN + 1]).unwrap();
    assert!(matches!(
        KeyFile::read(&long, Entropy::Fixed(0)),
        Err(KeyFileError::Read(PathError {
            error: ReadError::TooLarge { .. },
            ..
        }))
    ));
    // Others may read it: refused before its key is read, so text that is no key is refused as
    // exposed too, and a file of such a key is no missing one, which a new key would replace.
    for file in [&file, &bad] {
        file.expose();
        assert!(matches!(
            KeyFile::read(file, Entropy::Fixed(0)),
            Err(KeyFileError::Read(PathError {
                error: ReadError::Exposed(_),
                ..
            }))
        ));
    }
    assert!(matches!(
        KeyFile::read_or_create(&file, Entropy::Fixed(9)),
        Err(KeyFileError::Read(PathError {
            error: ReadError::Exposed(_),
            ..
        }))
    ));
}

#[test]
fn two_writers_that_make_one_key_file_at_once_share_one_key() {
    // Each draws its own key, of 1s or 2s; one creates the file, and the other reads its key.
    let test_dir = TestDir::new();
    let file = SecretFile::at(test_dir.path("player.nsec"));
    let fills = [Entropy::Fixed(1), Entropy::Fixed(2)];
    let keys = Race::run(2, |index| {
        *KeyFile::read_or_create(&file, fills[index])
            .unwrap()
            .secret_bytes()
    });
    assert_eq!(keys[0], keys[1]);
    assert!([[1; 32], [2; 32]].contains(&keys[0]));
    let read = KeyFile::read(&file, Entropy::Fixed(0)).unwrap();
    assert_eq!(*read.secret_bytes(), keys[0]);
}
