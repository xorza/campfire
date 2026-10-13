use campfire_script::rhai::ImmutableString;

use crate::mode::mode_spec::ModeSpec;
use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_arg::ScriptArg;
use crate::values::engine_enum::EngineEnum;
use crate::values::names::NameId;

/// A choice of the mode, by its place among the mode's choices, in the order of their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChoiceId(u16);

impl NameId for ChoiceId {
    type Scope = ();
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> ChoiceId {
        ChoiceId(u16::try_from(index).expect("choices fit u16"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

/// A choice a script names is one of the mode's; a match with no mode has none to name.
impl ScriptArg for ChoiceId {
    type Raw = ImmutableString;
    type Int = ImmutableString;
    const NAMES: Option<NameKind> = Some(NameKind::Choice);
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = false;

    fn take(raw: ImmutableString, ctx: &ArgCtx<'_>) -> Checked<ChoiceId> {
        let book = ModeSpec::of_or_fail(ctx.get())?;
        let choice = book.choices.names().named(raw.as_str());
        Ok(choice.ok_or_else(|| ApiError::Unknown(NameKind::Choice).fail())?)
    }

    fn take_int(raw: ImmutableString, ctx: &ArgCtx<'_>) -> Checked<ChoiceId> {
        ChoiceId::take(raw, ctx)
    }
}
