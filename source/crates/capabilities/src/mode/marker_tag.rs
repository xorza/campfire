use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::Checked;
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_arg::TextArg;
use crate::scripts::script_text::ScriptText;

/// A tag of the map's markers, as a script names it to find the markers with it: no marker
/// needs to hold it.
#[derive(Debug, Clone)]
pub(crate) struct MarkerTag(pub(crate) ScriptText);

impl TextArg for MarkerTag {
    const NAMES: NameKind = NameKind::MarkerTag;

    fn take_text(text: ScriptText, _: &ArgCtx<'_>) -> Checked<MarkerTag> {
        Ok(MarkerTag(text))
    }
}
