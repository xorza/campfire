use campfire_common::DeclaredName;
use derive_more::Display;

/// A unit type that stands, as a book error names it: the avatar its package stands as, or a
/// type by the name its package declares.
#[derive(Debug, Display, Clone, PartialEq, Eq)]
pub enum TypePlace {
    #[display("the avatar")]
    Avatar,
    #[display("unit type {_0}")]
    Declared(DeclaredName),
}
