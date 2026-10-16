use campfire_script::{NameKind, name_script_arg};
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

// A choice a script names is one of the mode's, which only the mode's calls name.
name_script_arg!(ChoiceId, NameKind::Choice, |ctx, name| {
    ctx.view().names().named(name)
});
