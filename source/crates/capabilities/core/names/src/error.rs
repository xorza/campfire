use campfire_values::NameKind;
use thiserror::Error;

/// A name that data gives of a kind the match does not declare: what a book refuses as it
/// resolves the name.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("no {of} {name:?}")]
pub struct UnknownName {
    pub of: NameKind,
    pub name: String,
}

impl UnknownName {
    /// The name `name`, which names no `of`.
    pub fn new(of: NameKind, name: &(impl ToString + ?Sized)) -> Self {
        Self {
            of,
            name: name.to_string(),
        }
    }
}
