use std::ops::Deref;

use rhai::ImmutableString;

/// A text a script reads or gives, shared, so a copy counts a reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptText(ImmutableString);

impl ScriptText {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether `other` is this very text, shared, not only one of the same letters.
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }

    pub(super) const fn of_engine(text: ImmutableString) -> Self {
        Self(text)
    }

    pub(super) fn into_engine(self) -> ImmutableString {
        self.0
    }
}

impl Deref for ScriptText {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for ScriptText {
    fn from(text: &str) -> Self {
        Self(text.into())
    }
}

impl From<String> for ScriptText {
    fn from(text: String) -> Self {
        Self(text.into())
    }
}
