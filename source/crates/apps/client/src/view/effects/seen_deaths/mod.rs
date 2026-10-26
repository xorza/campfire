use std::time::Duration;

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

/// A death whose middle or end is still to play: the death, its place the latest its drawing
/// stood at, and the app time its middle plays at, none once it played or for a death of no
/// middle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DeathUnderWay {
    pub(crate) death: SeenDeath,
    pub(crate) middle: Option<Duration>,
}

/// Where a dying unit stands in a frame: dead still, living again, or gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnitNow {
    Dead,
    Living,
    Gone,
}

/// What a death under way does in a frame: waits, plays its middle, plays its end and ends, or
/// ends with nothing more, its unit living again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeathStep {
    Waits,
    Middle,
    End,
    Ends,
}

impl DeathUnderWay {
    /// Its step in a frame at `now` of app time, its unit as `unit` stands and its drawing at
    /// `place`, which it takes as its latest: its middle once its time came, then its end as its
    /// unit goes.
    pub(crate) fn step(
        &mut self,
        unit: UnitNow,
        place: Option<Affine3A>,
        now: Duration,
    ) -> DeathStep {
        match unit {
            UnitNow::Living => DeathStep::Ends,
            UnitNow::Gone => DeathStep::End,
            UnitNow::Dead => {
                if let Some(place) = place {
                    self.death.place = place;
                }
                if self.middle.take_if(|middle| *middle <= now).is_some() {
                    DeathStep::Middle
                } else {
                    DeathStep::Waits
                }
            }
        }
    }
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
