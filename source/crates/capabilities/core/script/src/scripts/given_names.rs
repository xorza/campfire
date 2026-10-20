use campfire_values::NameKind;

/// The names a form gives, or a hook passes in an argument: of one kind, one name or a list of
/// them, which the load checks each literal a script compares them with against
/// (06-script-api.md, A name's kind reaches lists, hook arguments and variables).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GivenNames {
    pub kind: NameKind,
    pub shape: NameShape,
}

/// Whether a value is one name or a list of names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameShape {
    One,
    List,
}

impl GivenNames {
    /// One name of `kind`.
    pub const fn one(kind: NameKind) -> Self {
        Self {
            kind,
            shape: NameShape::One,
        }
    }

    /// A list of names of `kind`.
    pub const fn list(kind: NameKind) -> Self {
        Self {
            kind,
            shape: NameShape::List,
        }
    }

    /// The names an item of it gives: itself for one name, one name of its kind for a list's.
    #[must_use]
    pub const fn item(self) -> Self {
        Self::one(self.kind)
    }
}
