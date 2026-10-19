use campfire_math::Vec3;
use campfire_values::EngineEnum;

use crate::scripts::api_holder::ScriptHandle;
use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_arg::ScriptArg;
use campfire_values::NameKind;

/// A vector a script gives a call as a place: each of its coordinates within the world's bound,
/// `Vec3::BOUND`, as every place of the sim lies. One past it fails the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptPosition(Vec3);

impl ScriptPosition {
    pub const fn get(self) -> Vec3 {
        self.0
    }

    /// `point`, or the refusal of one past the world's bound.
    fn checked(point: Vec3) -> Checked<Self> {
        if !point.within_bound() {
            return Err(ApiError::PastBound.fail().into());
        }
        Ok(Self(point))
    }
}

impl ScriptArg for ScriptPosition {
    type Raw = Vec3;
    type Int = Vec3;
    const NAME_KIND: Option<NameKind> = None;
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = false;
    const TYPE: &'static str = <Vec3 as ScriptHandle>::NAME;

    fn take(raw: Vec3, _: &ArgCtx<'_>) -> Checked<Self> {
        Self::checked(raw)
    }

    fn take_int(raw: Vec3, _: &ArgCtx<'_>) -> Checked<Self> {
        Self::checked(raw)
    }
}
