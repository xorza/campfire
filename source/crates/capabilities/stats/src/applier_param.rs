use campfire_values::NameId;

/// A name of a param that a modifier reads from the ability that applies it, as the match's
/// modifiers name it, by its place among those names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApplierParam(u16);

impl NameId for ApplierParam {
    type Scope = ();
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> Self {
        Self(u16::try_from(index).expect("applier params fit u16"))
    }

    fn index(self) -> usize {
        usize::from(self.0)
    }
}
