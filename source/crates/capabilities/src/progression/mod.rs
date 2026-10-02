use std::collections::BTreeMap;

use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::{EntityRef, Mut, World};
use campfire_sim::{EntityIndex, StateRegistry};

use crate::progression::experience::Experience;
use crate::progression::level_ups::{LevelUp, LevelUps};
use crate::progression::progression_effect::ProgressionEffect;
use crate::progression::track_book::TrackBook;
use crate::progression::track_data::TrackData;
use crate::progression::track_set::TrackSet;
use crate::stats::level::Level;
use crate::units::script_view::{RowFill, View};
use crate::values::declared_name::DeclaredName;

pub(crate) mod experience;
pub(crate) mod level_ups;
pub(crate) mod progression_api;
pub(crate) mod progression_effect;
pub(crate) mod track_book;
pub(crate) mod track_data;
pub(crate) mod track_id;
pub(crate) mod track_set;

/// The `progression` capability: experience on tracks, and the levels it reaches.
#[derive(Debug)]
pub struct Progression;

impl Progression {
    /// Adds progression to a match, with no track until the mode loads its own.
    pub fn install(world: &mut World, _: &mut Schedule, registry: &mut StateRegistry) {
        world.non_send::<View>().add_source(fill_row);
        world.insert_resource(TrackBook::default());
        world.insert_resource(LevelUps::default());
        registry.register_component::<Experience>();
        registry.register_resource::<LevelUps>();
    }

    /// Loads the mode's `tracks`, which the package load checked, into the match.
    pub fn load(world: &mut World, tracks: &BTreeMap<DeclaredName, TrackData>) {
        let book = TrackBook::new(tracks);
        world.non_send::<View>().set_track_names(book.names());
        world.insert_resource(book);
    }

    /// The tracks of `names`, each one the loaded tracks hold; none for no name, in a match
    /// without progression too.
    pub fn tracks(world: &World, names: &[DeclaredName]) -> TrackSet {
        TrackSet::of(names.iter().map(|name| {
            world
                .resource::<TrackBook>()
                .id(name)
                .expect("the load checked a unit type's tracks")
        }))
    }

    /// Applies `effect`: experience raises its track's level, each level reached joins the
    /// tick's level-ups, and a level reached on the `level` track becomes the unit's level.
    pub(crate) fn apply(world: &mut World, effect: ProgressionEffect) {
        match effect {
            ProgressionEffect::AddXp {
                unit,
                track,
                amount,
            } => {
                let entity = world
                    .resource::<EntityIndex>()
                    .get(unit)
                    .expect("a unit given experience exists");
                world.resource_scope(|world, book: Mut<'_, TrackBook>| {
                    let raised = world
                        .get_mut::<Experience>(entity)
                        .expect("a unit given experience has tracks")
                        .add(track, amount, &book);
                    if raised.to == raised.from {
                        return;
                    }
                    let reached = (raised.from.get() + 1..=raised.to.get())
                        .map(|level| Level::new(level).expect("a level past another"));
                    let mut level_ups = world.resource_mut::<LevelUps>();
                    level_ups
                        .0
                        .extend(reached.map(|level| LevelUp { unit, track, level }));
                    if book.level_track() == Some(track) {
                        *world
                            .get_mut::<Level>(entity)
                            .expect("a unit with the level track has a level") = raised.to;
                    }
                });
            }
        }
    }
}

/// Fills a row of the script view with the tracks a unit has.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    fill.row.tracks = unit
        .get::<Experience>()
        .map_or(TrackSet::default(), Experience::tracks);
}

#[cfg(test)]
mod tests;
