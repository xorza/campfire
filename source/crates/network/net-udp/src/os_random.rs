//! The OS's random bytes, for every secret an OS endpoint draws.

use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroizing;

/// 32 random bytes from the OS.
pub(crate) fn secret() -> Zeroizing<[u8; 32]> {
    let mut secret = Zeroizing::new([0; 32]);
    SystemRandom::new()
        .fill(secret.as_mut_slice())
        .expect("the OS gives random bytes");
    secret
}
