use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::{EntityRef, Mut, World};
use campfire_math::Tick;
use campfire_sim::{EntityIndex, StateRegistry};

use crate::progression::experience::Experience;
use crate::progression::level_ups::{LevelUp, LevelUps};
use crate::progression::progression_effect::ProgressionEffect;
use crate::progression::track_book::TrackBook;

use crate::progression::track_set::TrackSet;
use crate::progression::tracks_column::TracksColumn;
use crate::scripts::frame::Frame;
use crate::stats::level::Level;
use crate::units::row_fill::RowFill;
use crate::units::script_view::View;

pub(crate) mod experience;
pub(crate) mod level_ups;
pub(crate) mod progression_api;
pub(crate) mod progression_effect;
pub(crate) mod track_book;
pub(crate) mod track_data;
pub(crate) mod track_set;
pub(crate) mod tracks_column;

/// The `progression` capability: experience on tracks, and the levels it reaches.
#[derive(Debug)]
pub struct Progression;

impl Progression {
    /// Adds progression to a match, with no track until the mode loads its own.
    pub fn install(world: &mut World, _: &mut Schedule, registry: &mut StateRegistry) {
        let view = world.non_send::<View>();
        view.add_column(TracksColumn::default());
        view.add_source(fill_row);
        world.insert_resource(TrackBook::default());
        world.insert_resource(LevelUps::default());
        registry.register_component::<Experience>();
        registry.register_resource::<LevelUps>();
    }

    /// Applies the next progression effect the call in `frame` queued.
    pub(crate) fn apply_next(world: &mut World, frame: &mut Frame, _: Tick) {
        Progression::apply(world, frame.effects.take::<ProgressionEffect>());
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
                    let mut carrier = world.entity_mut(entity);
                    let (mut experience, mut level) = carrier
                        .get_components_mut::<(&mut Experience, Option<&mut Level>)>()
                        .expect("a unit given experience has tracks");
                    // The unit's level counts as changed only when it rises, as its stats are
                    // derived again when it changes.
                    let unit_level = level
                        .as_mut()
                        .map(DetectChangesMut::bypass_change_detection);
                    let raised = experience.add(track, amount, &book, unit_level);
                    if raised.to == raised.from {
                        return;
                    }
                    if let Some(level) = &mut level
                        && book.level_track() == Some(track)
                    {
                        level.set_changed();
                    }
                    let reached = (raised.from.get() + 1..=raised.to.get())
                        .map(|level| Level::new(level).expect("a level past another"));
                    let mut level_ups = world.resource_mut::<LevelUps>();
                    level_ups
                        .0
                        .extend(reached.map(|level| LevelUp { unit, track, level }));
                });
            }
        }
    }
}

/// Fills a row of the script view with the tracks a unit has.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    let tracks = unit
        .get::<Experience>()
        .map_or(TrackSet::default(), Experience::tracks);
    fill.column::<TracksColumn>().push(tracks);
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::progression::Progression;
    use crate::progression::track_book::TrackBook;
    use crate::progression::track_data::TrackData;
    use crate::progression::tracks_column::TracksColumn;
    use crate::units::script_view::View;
    use crate::values::declared_name::DeclaredName;
    use bevy_ecs::world::World;
    use std::collections::BTreeMap;

    impl Progression {
        /// Loads the mode's `tracks`, which the package load checked, into the match.
        pub(crate) fn load(world: &mut World, tracks: &BTreeMap<DeclaredName, TrackData>) {
            let book = TrackBook::new(tracks);
            TracksColumn::share(world.non_send::<View>(), book.clone());
            world.insert_resource(book);
        }
    }
}

#[cfg(test)]
mod tests;
