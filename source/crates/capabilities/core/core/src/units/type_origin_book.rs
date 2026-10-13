use bevy_ecs::resource::Resource;
use campfire_sim::Capability;

use crate::declare::book::Book;
use crate::scripts::frame::Frame;
use crate::units::unit_type::UnitType;
use crate::units::view::View;

/// Where each unit type of the match comes from, by type: the package that declares it, by its
/// place in the mode's packages, the mode's first, and its name there, as its client data names
/// it. Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeOriginBook {
    origins: Vec<TypeOrigin>,
}

/// A unit type's package, by its place in the mode's packages, and its name in that package: its
/// key in the package's unit types, or, for an avatar, the package's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeOrigin {
    pub package: u16,
    pub name: Box<str>,
}

impl FromIterator<TypeOrigin> for TypeOriginBook {
    fn from_iter<I: IntoIterator<Item = TypeOrigin>>(origins: I) -> TypeOriginBook {
        TypeOriginBook {
            origins: origins.into_iter().collect(),
        }
    }
}

impl<const N: usize> From<[TypeOrigin; N]> for TypeOriginBook {
    fn from(origins: [TypeOrigin; N]) -> TypeOriginBook {
        origins.into_iter().collect()
    }
}

impl TypeOriginBook {
    pub fn push(&mut self, origin: TypeOrigin) {
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

impl Book for TypeOriginBook {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View, _: Option<&mut Frame>) {}
}
