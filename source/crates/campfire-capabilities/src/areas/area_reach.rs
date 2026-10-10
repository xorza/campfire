use bevy_ecs::system::{Res, SystemParam};
use campfire_math::Num;

use crate::areas::area_spec::AreaSpec;
use crate::units::by_type::ByType;
use crate::units::unit_type::UnitType;

/// How far each area unit reaches, as its type's spec gives it, for a reader outside the sim, as
/// the client draws it.
#[derive(SystemParam, Debug)]
pub struct AreaReach<'w> {
    specs: Res<'w, ByType<AreaSpec>>,
}

impl AreaReach<'_> {
    /// The radius an area of `unit_type` reaches; none for a type that is no area's.
    pub fn radius(&self, unit_type: UnitType) -> Option<Num> {
        Some(self.specs.get(unit_type)?.radius)
    }
}
