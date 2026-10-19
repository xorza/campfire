use campfire_math::Num;
use campfire_values::EngineEnum;

use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_arg::ScriptArg;
use campfire_values::NameKind;

/// A radius a script gives a query or a reveal: a number not below 0. One below 0 fails the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptRadius(Num);

impl ScriptRadius {
    pub const fn get(self) -> Num {
        self.0
    }

    /// `radius`, or the refusal of one below 0.
    fn checked(radius: Num) -> Checked<Self> {
        if radius < Num::ZERO {
            return Err(ApiError::NegativeRadius.fail().into());
        }
        Ok(Self(radius))
    }
}

impl ScriptArg for ScriptRadius {
    type Raw = Num;
    type Int = i64;
    const NAME_KIND: Option<NameKind> = None;
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = true;
    const TYPE: &'static str = "Num";

    fn take(raw: Num, ctx: &ArgCtx<'_>) -> Checked<Self> {
        Self::checked(Num::take(raw, ctx)?)
    }

    fn take_int(raw: i64, ctx: &ArgCtx<'_>) -> Checked<Self> {
        Self::checked(Num::take_int(raw, ctx)?)
    }
}
