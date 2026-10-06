use std::time::{SystemTime, UNIX_EPOCH};

/// What the operating system gives a binary's setups: its clock and its random bytes.
#[derive(Debug)]
pub struct Os;

impl Os {
    /// Fills `bytes` with the OS's random bytes: a setup's source of entropy.
    pub fn fill(bytes: &mut [u8; 32]) {
        getrandom::fill(bytes).expect("the OS gives random bytes");
    }

    /// Unix seconds: a setup's clock.
    pub fn unix_now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after 1970")
            .as_secs()
    }
}
