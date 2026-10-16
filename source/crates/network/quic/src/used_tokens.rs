use std::collections::BTreeSet;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use quinn_proto::{TokenLog, TokenReuseError};

/// The `NEW_TOKEN` tokens a server took, each until it expires, so each validates one address once
/// and a party that saw it pass cannot use it again (15-quic-transport.md, Retry, tokens and resets
/// after a restart). `quinn-proto` asks it for each token that decrypts, names the client's IP and
/// has not expired; it holds no token past its expiry, when `quinn-proto` refuses the token by
/// itself.
#[derive(Debug, Default)]
pub(crate) struct UsedTokens {
    used: Mutex<BTreeSet<UsedToken>>,
}

/// A token the log took: when it expires, and the random nonce `quinn-proto` gave it. Its order
/// is its expiry's first, so the log's oldest come first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct UsedToken {
    expires: SystemTime,
    nonce: u128,
}

impl TokenLog for UsedTokens {
    fn check_and_insert(
        &self,
        nonce: u128,
        issued: SystemTime,
        lifetime: Duration,
    ) -> Result<(), TokenReuseError> {
        let token = UsedToken {
            expires: issued + lifetime,
            nonce,
        };
        let mut used = self
            .used
            .lock()
            .expect("no thread panics while it holds the used tokens");
        // The token is looked up before the expired ones go, so a clock that moved between
        // `quinn-proto`'s check and this one takes no used token out before its own reuse.
        if used.contains(&token) {
            return Err(TokenReuseError);
        }
        let now = SystemTime::now();
        while used.first().is_some_and(|oldest| oldest.expires < now) {
            used.pop_first();
        }
        used.insert(token);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_works_once_and_its_record_goes_once_it_expired() {
        let log = UsedTokens::default();
        let now = SystemTime::now();
        let minute = Duration::from_secs(60);
        assert!(log.check_and_insert(1, now, minute).is_ok());
        assert!(log.check_and_insert(1, now, minute).is_err());
        // Another nonce, or the same nonce issued at another time, is another token; that one
        // expired 30 s ago.
        assert!(log.check_and_insert(2, now, minute).is_ok());
        let half = Duration::from_secs(30);
        assert!(log.check_and_insert(1, now - minute - half, minute).is_ok());
        assert_eq!(log.used.lock().unwrap().len(), 3);
        // Each check takes the records that expired before it: the next one takes nonce 1's that
        // expired 30 s ago, and keeps its own token, which expired a minute ago; the one after
        // takes that.
        assert!(log.check_and_insert(3, now - 2 * minute, minute).is_ok());
        assert_eq!(log.used.lock().unwrap().len(), 3);
        assert!(log.check_and_insert(4, now, minute).is_ok());
        let left: Vec<u128> = log
            .used
            .lock()
            .unwrap()
            .iter()
            .map(|token| token.nonce)
            .collect();
        assert_eq!(left, [1, 2, 4]);
    }
}
