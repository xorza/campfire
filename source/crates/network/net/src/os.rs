use std::time::{SystemTime, UNIX_EPOCH};

/// What a setup takes from the operating system, or from a test in its place: the clock and the
/// random bytes.
#[derive(Debug, Clone)]
pub struct Os {
    pub clock: Clock,
    pub entropy: Entropy,
}

/// The Unix seconds a server checks a delegation's expiry against, and a client makes one at.
#[derive(Debug, Clone)]
pub enum Clock {
    System,
    /// The seconds a test sets, which every holder of a clone reads.
    #[cfg(feature = "internals")]
    Manual(std::sync::Arc<std::sync::atomic::AtomicU64>),
}

/// What fills a key, a seed, a challenge or BIP-340's auxiliary randomness.
#[derive(Debug, Clone, Copy)]
pub enum Entropy {
    System,
    /// Every byte this one, so a test's keys and signatures repeat.
    #[cfg(feature = "internals")]
    Fixed(u8),
}

impl Os {
    /// The operating system's clock and random bytes, which a binary's setups take.
    pub const SYSTEM: Os = Os {
        clock: Clock::System,
        entropy: Entropy::System,
    };
}

impl Clock {
    pub fn unix_now(&self) -> u64 {
        match self {
            Clock::System => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("the clock is after 1970")
                .as_secs(),
            #[cfg(feature = "internals")]
            Clock::Manual(seconds) => seconds.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

impl Entropy {
    pub fn fill(self, bytes: &mut [u8; 32]) {
        match self {
            Entropy::System => getrandom::fill(bytes).expect("the OS gives random bytes"),
            #[cfg(feature = "internals")]
            Entropy::Fixed(byte) => bytes.fill(byte),
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::os::Clock;

    impl Clock {
        /// A clock that reads `seconds` until a test sets it.
        pub fn manual(seconds: u64) -> Clock {
            Clock::Manual(Arc::new(AtomicU64::new(seconds)))
        }

        /// Sets the seconds every clone of this manual clock reads.
        pub fn set(&self, seconds: u64) {
            let Clock::Manual(now) = self else {
                panic!("only a manual clock is set");
            };
            now.store(seconds, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::os::{Clock, Entropy};

    #[test]
    fn a_manual_clock_reads_what_any_clone_sets_and_fixed_entropy_fills_its_byte() {
        let clock = Clock::manual(10);
        let held = clock.clone();
        assert_eq!(held.unix_now(), 10);
        clock.set(25);
        assert_eq!(held.unix_now(), 25);
        let mut bytes = [0; 32];
        Entropy::Fixed(7).fill(&mut bytes);
        assert_eq!(bytes, [7; 32]);
    }
}
