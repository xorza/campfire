use campfire_common::Ticks;
use campfire_sim::MatchRate;
use campfire_values::EngineEnum;

use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_arg::ScriptArg;
use crate::scripts::script_ticks::ScriptTicks;
use campfire_values::NameKind;

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

impl ScriptArg for ScriptLasting {
    type Raw = i64;
    type Int = i64;
    const NAME_KIND: Option<NameKind> = None;
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = false;
    const TYPE: &'static str = "ms";

    fn take(ms: i64, ctx: &ArgCtx<'_>) -> Checked<Self> {
        Ok(Self::of(ctx.get().view().rate(), ms).map_err(ApiError::fail)?)
    }

    fn take_int(ms: i64, ctx: &ArgCtx<'_>) -> Checked<Self> {
        Self::take(ms, ctx)
    }
}
