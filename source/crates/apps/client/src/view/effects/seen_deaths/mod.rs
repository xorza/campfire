use bevy::ecs::entity::Entity;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::query::Allow;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, ResMut};
use bevy::math::Affine3A;
use bevy::transform::components::GlobalTransform;
use campfire_sim::Unpredicted;
use campfire_units::Dead;
use campfire_values::UnitType;

use crate::view::drawing::Drawing;

/// The deaths the client saw since the effects last played them, each as `Dead` reached its
/// unit, which a frame that also despawns the unit leaves no query to see
/// (04-capabilities/combat.md, A death replicates before its unit goes).
#[derive(Resource, Debug, Default)]
pub(crate) struct SeenDeaths(pub(crate) Vec<SeenDeath>);

/// A death seen: its unit, the unit's type, its drawing's root and where the root stood.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SeenDeath {
    pub(crate) unit: Entity,
    pub(crate) unit_type: UnitType,
    pub(crate) root: Entity,
    pub(crate) place: Affine3A,
}

impl SeenDeaths {
    /// The deaths to play, which it leaves: each but one whose unit `revived` of the frame names
    /// and that `stands` still, as a rollback across a predicted death takes `Dead` away and gives
    /// it back in one frame; a unit despawned since, which `revived` names too, plays.
    pub(crate) fn played<'a>(
        &'a mut self,
        revived: &'a [Entity],
        stands: impl Fn(Entity) -> bool + 'a,
    ) -> impl Iterator<Item = SeenDeath> + 'a {
        self.0
            .drain(..)
            .filter(move |death| revived.binary_search(&death.unit).is_err() || !stands(death.unit))
    }

    /// Takes the death of the unit that `Dead` reached, of a drawn unit alone.
    pub(crate) fn see(
        added: On<'_, '_, Add<Dead>>,
        units: Query<'_, '_, (&UnitType, &Drawing), Allow<Unpredicted>>,
        places: Query<'_, '_, &GlobalTransform>,
        mut deaths: ResMut<'_, Self>,
    ) {
        let unit = added.entity;
        let Ok((&unit_type, drawing)) = units.get(unit) else {
            return;
        };
        let root = drawing.root();
        if let Ok(place) = places.get(root) {
            deaths.0.push(SeenDeath {
                unit,
                unit_type,
                root,
                place: place.affine(),
            });
        }
    }
}

#[cfg(test)]
mod tests;
