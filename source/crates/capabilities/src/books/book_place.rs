use campfire_values::DeclaredName;
use derive_more::Display;

use crate::books::type_place::TypePlace;

/// Where in a package a book finds a name it resolves, as a book error names it.
#[derive(Debug, Display, Clone, PartialEq, Eq)]
pub enum BookPlace {
    #[display("{_0}")]
    UnitType(TypePlace),
    #[display("action {_0}")]
    Action(DeclaredName),
    #[display("modifier {_0}")]
    Modifier(DeclaredName),
    /// The mode's pool of that name.
    #[display("pool {_0}")]
    Pool(DeclaredName),
    /// The mode's item type of that id.
    #[display("item {_0}")]
    Item(DeclaredName),
    /// The mode's `[combat]`.
    #[display("the mode's [combat]")]
    Combat,
}
