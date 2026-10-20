use std::time::{SystemTime, UNIX_EPOCH};

/// The Unix seconds a server checks a delegation's expiry against, and a client makes one at.
#[derive(Debug, Clone)]
pub enum UnixClock {
    System,
    /// The seconds a test sets, which every holder of a clone reads.
    #[cfg(feature = "internals")]
    Manual(std::sync::Arc<std::sync::atomic::AtomicU64>),
}

impl UnixClock {
    pub fn unix_now(&self) -> u64 {
        match self {
            Self::System => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("the clock is after 1970")
                .as_secs(),
            #[cfg(feature = "internals")]
            Self::Manual(seconds) => seconds.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::unix_clock::UnixClock;

    impl UnixClock {
        /// A clock that reads `seconds` until a test sets it.
        pub fn manual(seconds: u64) -> Self {
            Self::Manual(Arc::new(AtomicU64::new(seconds)))
        }

        /// Sets the seconds every clone of this manual clock reads.
        pub fn set(&self, seconds: u64) {
            let Self::Manual(now) = self else {
                panic!("only a manual clock is set");
            };
            now.store(seconds, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::unix_clock::UnixClock;

    #[test]
    fn a_manual_clock_reads_what_any_clone_sets() {
        let clock = UnixClock::manual(10);
        let held = clock.clone();
        assert_eq!(held.unix_now(), 10);
        clock.set(25);
        assert_eq!(held.unix_now(), 25);
    }
}
