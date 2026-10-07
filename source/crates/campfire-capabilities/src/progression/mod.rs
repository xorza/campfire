use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::query::ROQueryItem;
use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::{Mut, World};
use campfire_sim::{Capability, EntityIndex, StateRegistry};

use crate::actions::effect_queues::EffectQueues;
use crate::progression::experience::Experience;
use crate::progression::level_ups::{LevelUp, LevelUps};
use crate::progression::points::Points;
use crate::progression::progression_effect::ProgressionEffect;
use crate::progression::track_book::TrackBook;

use crate::progression::progression_column::ProgressionColumn;
use crate::stats::level::Level;
use crate::units::row_fill::RowFill;
use crate::units::script_view::View;

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
        view.add_source::<RowParts, _>(world, fill_row);
        world.insert_resource(TrackBook::default());
        world.insert_resource(LevelUps::default());
        world
            .resource_mut::<EffectQueues>()
            .register(Capability::Progression, ProgressionEffect::queue_listed);
        registry.register_component::<Experience>();
        registry.register_component::<Points>();
        registry.register_resource::<LevelUps>();
    }

    /// Applies `effect`: experience raises its track's level, each level reached joins the
    /// tick's level-ups, and a level reached on the `level` track becomes the unit's level and
    /// gives it a point.
    fn apply(world: &mut World, effect: ProgressionEffect) {
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
                    let (mut experience, mut level, points) = carrier
                        .get_components_mut::<(
                            &mut Experience,
                            Option<&mut Level>,
                            Option<&mut Points>,
                        )>()
                        .expect("a unit given experience has tracks");
                    // The unit's level counts as changed only when it rises, as its stats are
                    // derived again when it changes.
                    let mut unit_level = level.as_deref().copied();
                    let raised = experience.add(track, amount, &book, unit_level.as_mut());
                    if let (Some(level), Some(value)) = (&mut level, unit_level) {
                        level.set_if_neq(value);
                    }
                    if raised.to == raised.from {
                        return;
                    }
                    if level.is_some() && book.level_track() == Some(track) {
                        points
                            .expect("a unit with the `level` track has points")
                            .gain(raised.to.get() - raised.from.get());
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

/// The parts of a unit progression reads into its row: its experience, its level, which is its
/// `level` track's, and its points.
type RowParts = (
    Option<&'static Experience>,
    Option<&'static Level>,
    Option<&'static Points>,
);

/// Fills a row of the script view with a unit's tracks, its progress on each, and its points.
fn fill_row(
    (experience, level, points): ROQueryItem<'_, '_, RowParts>,
    fill: &mut RowFill<'_, ProgressionColumn>,
) {
    fill.column.push(experience, level, points);
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::progression::Progression;
    use crate::progression::progression_column::ProgressionColumn;
    use crate::progression::track_book::TrackBook;
    use crate::progression::track_data::TrackData;
    use crate::units::script_view::View;
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
