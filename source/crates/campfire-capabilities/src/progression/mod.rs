use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_sim::{Capability, StateRegistry};

use crate::actions::effect_queues::EffectQueues;
use crate::progression::experience::Experience;
use crate::progression::level_ups::LevelUps;
use crate::progression::points::Points;
use crate::progression::progression_column::{ProgressionColumn, RowParts};
use crate::progression::progression_effect::ProgressionEffect;
use crate::progression::track_book::TrackBook;
use crate::units::view::View;

pub(crate) mod experience;
pub(crate) mod level_ups;
pub(crate) mod points;
pub(crate) mod progression_api;
pub(crate) mod progression_column;
pub(crate) mod progression_effect;
pub(crate) mod track_book;
pub(crate) mod track_data;
pub(crate) mod track_set;

/// The `progression` capability: experience on tracks, and the levels it reaches.
#[derive(Debug)]
pub struct Progression;

impl Progression {
    /// Adds progression to a match, with no track until the mode loads its own.
    pub fn install(world: &mut World, _: &mut Schedule, registry: &mut StateRegistry) {
        let view = world.non_send::<View>().clone();
        view.add_column(ProgressionColumn::default());
        view.add_source::<RowParts, _>(world, ProgressionColumn::fill_row);
        world.insert_resource(TrackBook::default());
        world.insert_resource(LevelUps::default());
        world
            .resource_mut::<EffectQueues>()
            .register(Capability::Progression, ProgressionEffect::queue_listed);
        registry.register_component::<Experience>();
        registry.register_component::<Points>();
        registry.register_resource::<LevelUps>();
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::progression::Progression;
    use crate::progression::progression_column::ProgressionColumn;
    use crate::progression::track_book::TrackBook;
    use crate::progression::track_data::TrackData;
    use crate::units::view::View;
    use crate::values::declared_name::DeclaredName;
    use bevy_ecs::world::World;
    use std::collections::BTreeMap;

    impl Progression {
        /// Loads the mode's `tracks`, which the package load checked, into the match.
        pub(crate) fn load(world: &mut World, tracks: &BTreeMap<DeclaredName, TrackData>) {
            let book = TrackBook::new(tracks);
            ProgressionColumn::share(world.non_send::<View>(), book.clone());
            world.insert_resource(book);
        }
    }
}

#[cfg(test)]
mod tests;
