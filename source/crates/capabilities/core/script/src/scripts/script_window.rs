use campfire_common::Ticks;
use campfire_sim::MatchRate;

use crate::scripts::error::ApiError;
use crate::scripts::script_arg::ms_script_arg;

/// A time a script gives in milliseconds as a window back from now: in ticks at the match's rate,
/// rounded up, and 0 for 0; every tick there is for a time too long to count. A negative time
/// fails the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptWindow(Ticks);

impl ScriptWindow {
    /// `ms` at `rate`, or why it is refused.
    pub fn of(rate: MatchRate, ms: i64) -> Result<Self, ApiError> {
        let ms = u64::try_from(ms).map_err(|_| ApiError::NegativeTime)?;
        Ok(Self(rate.window(ms)))
    }

    pub const fn get(self) -> Ticks {
        self.0
    }
}

ms_script_arg!(ScriptWindow);
