use campfire_common::Ticks;
use campfire_sim::MatchRate;

use crate::scripts::error::ApiError;
use crate::scripts::script_arg::ms_script_arg;
use crate::scripts::script_ticks::ScriptTicks;

/// A time a script gives in milliseconds for what must last, as a reveal or a knock back: more
/// than 0, in ticks as `ScriptTicks` gives them. A time of 0 fails the call, as one that
/// `ScriptTicks` refuses does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptLasting(Ticks);

impl ScriptLasting {
    /// `ms` at `rate`, or why it is refused.
    pub fn of(rate: MatchRate, ms: i64) -> Result<Self, ApiError> {
        if ms == 0 {
            return Err(ApiError::ZeroTime);
        }
        Ok(Self(ScriptTicks::of(rate, ms)?.get()))
    }

    pub const fn get(self) -> Ticks {
        self.0
    }
}

ms_script_arg!(ScriptLasting);
