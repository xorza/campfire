use std::fmt;

use crate::values::declared_name::DeclaredName;

/// A unit type that stands, as a book error names it: the avatar its package stands as, or a
/// type by the name its package declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypePlace {
    Avatar,
    Declared(DeclaredName),
}

impl fmt::Display for TypePlace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypePlace::Avatar => f.write_str("the avatar"),
            TypePlace::Declared(name) => write!(f, "unit type {name}"),
        }
    }
}
