use campfire_common::Ticks;
use campfire_sim::TickRate;
use campfire_values::EngineEnum;

use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_arg::ScriptArg;
use campfire_values::NameKind;

/// A time a script gives in milliseconds to take off a wait: in ticks at the match's rate,
/// rounded down, so a cut ends no wait before its time, and 0 for 0; every tick there is for a
/// time too long to count. A negative time fails the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptCut(Ticks);

impl ScriptCut {
    /// `ms` at `rate`, or why it is refused.
    pub fn of(rate: TickRate, ms: i64) -> Result<Self, ApiError> {
        let ms = u64::try_from(ms).map_err(|_| ApiError::NegativeTime)?;
        Ok(Self(rate.cut(ms)))
    }

    pub const fn get(self) -> Ticks {
        self.0
    }
}

impl ScriptArg for ScriptCut {
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
