use bevy_ecs::resource::Resource;

use crate::units::unit_type::UnitType;

/// Where each unit type of the match comes from, by type: the package that declares it, by its
/// place in the mode's packages, the mode's first, and its name there, as its client data names
/// it. Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeOrigins {
    origins: Vec<TypeOrigin>,
}

/// A unit type's package, by its place in the mode's packages, and its name in that package: its
/// key in the package's unit types, or, for an avatar, the package's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeOrigin {
    pub package: u16,
    pub name: Box<str>,
}

impl FromIterator<TypeOrigin> for TypeOrigins {
    fn from_iter<I: IntoIterator<Item = TypeOrigin>>(origins: I) -> TypeOrigins {
        TypeOrigins {
            origins: origins.into_iter().collect(),
        }
    }
}

impl TypeOrigins {
    pub(crate) fn push(&mut self, origin: TypeOrigin) {
        self.origins.push(origin);
    }

    /// The origin of `unit_type`, a type of the match.
    pub fn of(&self, unit_type: UnitType) -> &TypeOrigin {
        &self.origins[unit_type.index()]
    }

    /// Every type and its origin, in the order of the types.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (UnitType, &TypeOrigin)> {
        self.origins.iter().enumerate().map(|(at, origin)| {
            let index = u16::try_from(at).expect("a match's types fit u16");
            (UnitType::new(index), origin)
        })
    }
}
