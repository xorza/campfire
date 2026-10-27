use std::time::Duration;

use bevy::ecs::entity::Entity;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::query::Allow;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, ResMut};
use bevy::math::Affine3A;
use bevy::transform::components::GlobalTransform;
use campfire_combat::Decay;
use campfire_common::Tick;
use campfire_sim::Unpredicted;
use campfire_units::Dead;
use campfire_values::UnitType;

use crate::view::drawing::Drawing;

/// The deaths the client saw since the effects last played them, each as `Dead` reached its
/// unit, which a frame that also despawns the unit leaves no query to see
/// (04-capabilities/combat.md, A death replicates before its unit goes).
#[derive(Resource, Debug, Default)]
pub(crate) struct SeenDeaths(pub(crate) Vec<SeenDeath>);

/// A death seen: its unit, the unit's type, its drawing's root, where the root stood, and the
/// tick its unit decays at, none for a unit that stays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SeenDeath {
    pub(crate) unit: Entity,
    pub(crate) unit_type: UnitType,
    pub(crate) root: Entity,
    pub(crate) place: Affine3A,
    pub(crate) decay: Option<Tick>,
}

/// A death whose middle or end is still to play: the death, its place the latest its drawing
/// stood at, and the app time its middle plays at, none once it played or for a death of no
/// middle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DeathUnderWay {
    pub(crate) death: SeenDeath,
    pub(crate) middle: Option<Duration>,
}

/// Where a dying unit stands in a frame: dead still, living again, gone as its decay ends, or gone
/// from the client's sight before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnitNow {
    Dead,
    Living,
    Decayed,
    Left,
}

/// What a death under way does in a frame: waits, plays its middle, plays its end and ends, or
/// ends with nothing more, its unit living again or gone from the client's sight, where the fog
/// hides its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeathStep {
    Waits,
    Middle,
    End,
    Ends,
}

impl DeathUnderWay {
    /// How its unit, gone from the client, went, the client holding the server's state of the
    /// tick `received` before: as its decay ended, a despawn that comes with the state of its
    /// decay's tick or a later one, or else from the client's sight, as a unit of no decay always
    /// does (04-capabilities/combat.md, A death's decay reaches its clients).
    pub(crate) fn went(&self, received: Option<Tick>) -> UnitNow {
        match (self.death.decay, received) {
            (Some(decay), Some(received)) if received > decay => UnitNow::Decayed,
            _ => UnitNow::Left,
        }
    }

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
            UnitNow::Living | UnitNow::Left => DeathStep::Ends,
            UnitNow::Decayed => DeathStep::End,
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
        units: Query<'_, '_, (&UnitType, &Drawing, Option<&Decay>), Allow<Unpredicted>>,
        places: Query<'_, '_, &GlobalTransform>,
        mut deaths: ResMut<'_, Self>,
    ) {
        let unit = added.entity;
        let Ok((&unit_type, drawing, decay)) = units.get(unit) else {
            return;
        };
        let root = drawing.root();
        if let Ok(place) = places.get(root) {
            deaths.0.push(SeenDeath {
                unit,
                unit_type,
                root,
                place: place.affine(),
                decay: decay.map(|decay| decay.at),
            });
        }
    }
}

#[cfg(test)]
mod tests;
