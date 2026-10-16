use campfire_script::{ApiError, ArgCtx, Checked, NameKind, ScriptText, TextArg};
use campfire_values::NameId;

/// A choice of the mode, by its place among the mode's choices, in the order of their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChoiceId(u16);

impl NameId for ChoiceId {
    type Scope = ();
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> Self {
        Self(u16::try_from(index).expect("choices fit u16"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

/// A choice a script names is one of the mode's; a match with no mode has none to name.
impl TextArg for ChoiceId {
    const NAMES: NameKind = NameKind::Choice;

    fn take_text(text: ScriptText, ctx: &ArgCtx<'_>) -> Checked<Self> {
        let names = ctx.get().view().names();
        let names = names
            .table::<Self>()
            .ok_or_else(|| ApiError::NoMode.fail())?;
        let choice = names.named(text.as_str());
        Ok(choice.ok_or_else(|| ApiError::Unknown(NameKind::Choice).fail())?)
    }
}
