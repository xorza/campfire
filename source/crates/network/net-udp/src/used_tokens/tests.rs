use super::*;
use core::time::Duration;

fn at(secs: u64) -> Instant {
    Instant::from_elapsed(Duration::from_secs(secs))
}

/// A token works once; another nonce, or the same nonce with another expiry, is another token;
/// a record goes once its token expired, so the log holds the live ones alone.
#[test]
fn a_token_works_once_and_its_record_goes_once_it_expired() {
    let mut log = UsedTokens::default();
    assert!(log.take(1, at(60), at(0)));
    assert!(!log.take(1, at(60), at(1)));
    assert!(log.take(2, at(60), at(1)));
    assert!(log.take(1, at(30), at(1)));
    assert_eq!(log.used.len(), 3);
    assert!(log.take(3, at(90), at(30)));
    assert_eq!(
        log.used.iter().map(|&(_, nonce)| nonce).collect::<Vec<_>>(),
        [1, 2, 3]
    );
}
