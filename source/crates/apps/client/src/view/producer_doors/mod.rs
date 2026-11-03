use std::collections::BTreeMap;

use bevy::app::{App, Plugin, Startup};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res};
use campfire_declarations::ById;
use campfire_production::{DoorTimes, ProducerSpec};
use campfire_values::UnitType;

/// The door times of each type of producer of doors, as production's part of the looks gives
/// them as the client starts, so a door's look names no book of production's; none in a mode of
/// no production (02-engine-core.md, A client module list).
#[derive(Resource, Debug, Default)]
pub(crate) struct ProducerDoors(BTreeMap<UnitType, DoorTimes>);

impl ProducerDoors {
    /// The door times of `unit_type`, none for a type of no doors.
    pub(crate) fn of(&self, unit_type: UnitType) -> Option<DoorTimes> {
        self.0.get(&unit_type).copied()
    }

    /// The door times of each producer of `producers` that has doors.
    pub(crate) fn of_producers(producers: &ById<UnitType, ProducerSpec>) -> Self {
        let doors = producers
            .iter()
            .filter_map(|(unit_type, producer)| Some((unit_type, producer.doors?)));
        Self(doors.collect())
    }
}

/// Production's part of the looks: the door times of its producers.
#[derive(Debug)]
pub(crate) struct ProducerDoorsPlugin;

impl Plugin for ProducerDoorsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::read);
    }
}

impl ProducerDoorsPlugin {
    /// Reads the door times of the match's producers.
    fn read(producers: Res<'_, ById<UnitType, ProducerSpec>>, mut commands: Commands<'_, '_>) {
        commands.insert_resource(ProducerDoors::of_producers(&producers));
    }
}

#[cfg(test)]
mod tests;
