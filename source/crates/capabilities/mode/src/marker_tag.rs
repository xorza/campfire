use campfire_core::{ArgCtx, Checked, NameKind, ScriptText, TextArg};

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
