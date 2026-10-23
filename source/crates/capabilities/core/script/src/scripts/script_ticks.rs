use campfire_common::Ticks;
use campfire_sim::MatchRate;

use crate::scripts::error::ApiError;
use crate::scripts::script_arg::ms_script_arg;

/// A time a script gives in milliseconds, as how long something waits or lasts: in ticks at the
/// match's rate, rounded up, a tick at the least, so what starts also takes effect. A negative
/// time, or one too long to count, fails the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptTicks(Ticks);

impl ScriptTicks {
    /// `ms` at `rate`, or why it is refused.
    pub fn of(rate: MatchRate, ms: i64) -> Result<Self, ApiError> {
        let ms = u64::try_from(ms).map_err(|_| ApiError::NegativeTime)?;
        rate.duration(ms).map(Self).ok_or(ApiError::TimeTooLarge)
    }

    pub const fn get(self) -> Ticks {
        self.0
    }
}

ms_script_arg!(ScriptTicks);
