use std::borrow::Borrow;

use campfire_common::DeclaredName;
use derive_more::Display;

use crate::zero_hour::error::NameError;
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::import_name::ImportName;

/// The name of a unit type of the package, which keys each map of the stages' types: an object's
/// `ImportName`, or a projectile's of its own, checked once as a declared name. A map keyed by it
/// finds a type by the declared name the engine's data gives it.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TypeName(DeclaredName);

impl TypeName {
    /// The name of `object`'s unit type; an error past the bytes a declared name holds.
    pub(crate) fn of(object: &IniObject) -> Result<Self, NameError> {
        Self::new(&ImportName::object(object))
    }

    /// The type name `name` is; an error past the bytes a declared name holds.
    pub(crate) fn new(name: &ImportName) -> Result<Self, NameError> {
        name.declared().map(Self)
    }

    pub(crate) const fn declared(&self) -> &DeclaredName {
        &self.0
    }

    /// The id of `kind` of it, `<kind>_<name>`, as an action or a gather names its type.
    pub(crate) fn id(&self, kind: &str) -> Result<DeclaredName, NameError> {
        let id = format!("{kind}_{}", self.0);
        DeclaredName::new(&id).ok_or(NameError::TooLong(id))
    }
}

impl From<TypeName> for DeclaredName {
    fn from(name: TypeName) -> Self {
        name.0
    }
}

/// By the declared name alone, so its order, equality and hash are the declared name's.
impl Borrow<DeclaredName> for TypeName {
    fn borrow(&self) -> &DeclaredName {
        &self.0
    }
}

#[cfg(test)]
pub(crate) mod reach_in {
    use campfire_common::DeclaredName;

    use crate::zero_hour::type_name::TypeName;

    impl TypeName {
        /// The type name `text`, a declared name.
        pub(crate) fn named(text: &str) -> Self {
            Self(DeclaredName::new(text).unwrap())
        }

        pub(crate) fn as_str(&self) -> &str {
            self.0.as_str()
        }
    }
}
