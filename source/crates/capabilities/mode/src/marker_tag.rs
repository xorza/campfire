use campfire_script::{NameKind, name_script_arg};
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

// A marker tag a script names is one some marker of the map holds, which only the mode names.
name_script_arg!(MarkerTag, NameKind::MarkerTag, |ctx, name| {
    ctx.view().names().named(name)
});
