use campfire_script::{ApiError, ArgCtx, Checked, NameKind, ScriptText, TextArg};
use campfire_values::NameId;

/// A tag of the map's markers, by its place among the tags they hold, in the order of their
/// names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MarkerTag(u16);

impl NameId for MarkerTag {
    type Scope = ();
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> Self {
        Self(u16::try_from(index).expect("marker tags fit u16"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

/// A marker tag a script names is one some marker of the map holds; a match with no mode has
/// none to name.
impl TextArg for MarkerTag {
    const NAMES: NameKind = NameKind::MarkerTag;

    fn take_text(text: ScriptText, ctx: &ArgCtx<'_>) -> Checked<Self> {
        let names = ctx.get().view().names();
        let names = names
            .table::<Self>()
            .ok_or_else(|| ApiError::NoMode.fail())?;
        let tag = names.named(text.as_str());
        Ok(tag.ok_or_else(|| ApiError::Unknown(NameKind::MarkerTag).fail())?)
    }
}
