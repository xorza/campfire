use std::ops::Deref;

use campfire_script::rhai::ImmutableString;

/// A text a script reads or gives, shared, so a copy counts a reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptText(ImmutableString);

impl ScriptText {
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub(super) const fn of_engine(text: ImmutableString) -> ScriptText {
        ScriptText(text)
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
    fn from(text: &str) -> ScriptText {
        ScriptText(text.into())
    }
}

impl From<String> for ScriptText {
    fn from(text: String) -> ScriptText {
        ScriptText(text.into())
    }
}
