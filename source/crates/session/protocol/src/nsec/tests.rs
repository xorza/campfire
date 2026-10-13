use super::*;
use crate::harness::test_key::TestKey;

#[test]
fn a_key_round_trips_and_what_is_no_nsec_is_refused() {
    let key = TestKey::of(7);
    let text = Nsec::encode(&key);
    assert!(
        text.starts_with("nsec1") && text.ends_with('\n'),
        "{}",
        *text
    );
    // 63 characters, 4 + 1 + 52 + 6, and the newline, in the buffer sized for them; a key file
    // may hold 126 bytes.
    assert_eq!((text.len(), text.capacity()), (64, 64));
    assert_eq!(Nsec::MAX_FILE_LEN, 126);
    assert_eq!(Nsec::decode(text.as_bytes()).unwrap(), key);
    assert_eq!(
        Nsec::decode(format!("  {}  ", text.trim()).as_bytes()).unwrap(),
        key
    );
    // An npub, text that is no key, and bytes that are no text.
    for wrong in [
        &b"npub1sg6plzptd64u62a878hep2kev88swjh3tw00gjsfl8f237lmu63q0uf63m"[..],
        b"secret",
    ] {
        assert!(matches!(Nsec::decode(wrong), Err(NsecError::NotNsec(_))));
    }
    assert!(matches!(
        Nsec::decode(&[0xff, 0xfe]),
        Err(NsecError::NotText(_))
    ));
}
