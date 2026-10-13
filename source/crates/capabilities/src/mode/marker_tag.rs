use campfire_script::rhai::ImmutableString;

use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::Checked;
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_arg::ScriptArg;
use crate::values::engine_enum::EngineEnum;

/// A tag of the map's markers, as a script names it to find the markers with it: no marker
/// needs to hold it.
#[derive(Debug, Clone)]
pub(crate) struct MarkerTag(pub(crate) ImmutableString);

impl ScriptArg for MarkerTag {
    type Raw = ImmutableString;
    type Int = ImmutableString;
    const NAMES: Option<NameKind> = Some(NameKind::MarkerTag);
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = false;

    fn take(raw: ImmutableString, _: &ArgCtx<'_>) -> Checked<MarkerTag> {
        Ok(MarkerTag(raw))
    }

    fn take_int(raw: ImmutableString, _: &ArgCtx<'_>) -> Checked<MarkerTag> {
        Ok(MarkerTag(raw))
    }
}
