use core::time::Duration;

use campfire_net_core::Instant;

/// A handshake's first timer: the probe timeout before an RTT sample, 333 + 4 · 166.5 ms (RFC
/// 9002 §6.2.2, `kInitialRtt`), which doubles at each resend.
const HANDSHAKE_TIMER: Duration = Duration::from_millis(999);

/// When a handshake's flight goes again: [`HANDSHAKE_TIMER`] after it went, doubled for each
/// resend before.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResendTimer {
    at: Instant,
    resends: u32,
}

impl ResendTimer {
    /// The timer of a flight sent at `now`.
    pub(crate) fn start(now: Instant) -> Self {
        Self {
            at: now + HANDSHAKE_TIMER,
            resends: 0,
        }
    }

    /// The flight went again at `now` for a cause of its own, as a client's after a retry: the
    /// timer counts from it, at the period it had.
    pub(crate) fn restart(&mut self, now: Instant) {
        self.at = now + HANDSHAKE_TIMER;
    }

    /// The timer fired at `now` and the flight goes again: the next fires twice as late.
    pub(crate) fn back_off(&mut self, now: Instant) {
        self.resends += 1;
        self.at = now + HANDSHAKE_TIMER * 2_u32.saturating_pow(self.resends);
    }

    pub(crate) const fn at(&self) -> Instant {
        self.at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(millis: u64) -> Instant {
        Instant::from_elapsed(Duration::from_millis(millis))
    }

    /// From 0, the timer fires at 999 ms; each resend doubles the period, 1998 ms after the first
    /// at 1000 and 3996 after the second at 3000; a restart at 5000 counts 999 ms from it.
    #[test]
    fn each_resend_doubles_the_period() {
        let mut timer = ResendTimer::start(at(0));
        assert_eq!(timer.at(), at(999));
        timer.back_off(at(1000));
        assert_eq!(timer.at(), at(2998));
        timer.back_off(at(3000));
        assert_eq!(timer.at(), at(6996));
        timer.restart(at(5000));
        assert_eq!(timer.at(), at(5999));
    }
}
