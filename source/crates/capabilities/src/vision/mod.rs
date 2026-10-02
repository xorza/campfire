use std::ops::Range;

use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Commands, Local, Query, Res};
use bevy_ecs::world::{EntityRef, World};
use campfire_sim::{Position, SimSet, StateRegistry};

use crate::units::dead::Dead;
use crate::units::relations::Relations;
use crate::units::script_view::{RowFill, View};
use crate::units::team::Team;
use crate::units::team_set::TeamSet;
use crate::units::unit_tags::UnitTags;
use crate::values::grid::Grid;
use crate::vision::seen_by::SeenBy;
use crate::vision::sight::Sight;
use crate::vision::vision_grid::VisionGrid;
use crate::vision::vision_groups::VisionGroups;

pub(crate) mod seen_by;
pub(crate) mod sight;
pub(crate) mod vision_api;
pub(crate) mod vision_data;
pub(crate) mod vision_grid;
pub(crate) mod vision_groups;

/// The `vision` capability, for now its grid fog of war: what each vision group sees, which
/// decides what its teams' clients receive and what `find_visible`, `nearest_visible` and
/// `unit.can_see` return.
#[derive(Debug)]
pub struct Vision;

impl Vision {
    /// Adds vision to a match, on combat: in Vision, the last stage of a tick, each living unit
    /// with a sight reveals the grid cells around it to its vision group, and each unit learns the
    /// teams that see it. A match sees nothing until its mode gives the grid.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        if let Some(view) = world.get_non_send::<View>() {
            view.add_source(fill_row);
        }
        schedule.add_systems(see.in_set(SimSet::Vision));
        registry.register_component::<SeenBy>();
        registry.register_component::<Sight>();
    }

    /// Gives the match the map's `grid`, and the number of its teams.
    pub fn load_grid(world: &mut World, grid: Grid, teams: usize) {
        assert!(teams <= Team::LIMIT, "the mode's check limits the teams");
        world.insert_resource(VisionGrid { grid, teams });
    }

    /// The teams that see `unit`: those the last Vision stage found, or, before it ran, the
    /// unit's vision group under `relations`, as that stage would give it at the least; every
    /// team for an entity with no team, as a match without vision sees.
    fn seen_by(unit: &EntityRef<'_>, relations: &Relations) -> TeamSet {
        match (unit.get::<SeenBy>(), unit.get::<Team>()) {
            (Some(seen), _) => seen.get(),
            (None, Some(&team)) => relations.vision_group(team),
            (None, None) => TeamSet::ALL,
        }
    }
}

/// Fills a row of the script view with the teams that see the unit.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    fill.row.seen_by = Vision::seen_by(unit, fill.world.resource::<Relations>());
}

/// Reveals the cells each living unit with a sight sees to its vision group, and those each such
/// unit whose tags detect sees to its group's detection, then gives each unit the teams that see
/// it: its own group's, and those of each group whose cells hold it, or, for a unit its tags
/// hide, whose detection does. The groups follow the relations as they change.
fn see(
    (grid, relations): (Option<Res<'_, VisionGrid>>, Res<'_, Relations>),
    seers: Query<'_, '_, (&Position, &Team, &Sight, Option<&UnitTags>), Without<Dead>>,
    mut units: Query<
        '_,
        '_,
        (
            Entity,
            &Position,
            &Team,
            Option<&UnitTags>,
            Option<&mut SeenBy>,
        ),
    >,
    mut commands: Commands<'_, '_>,
    (mut groups, mut revealed, mut detected): (
        Local<'_, VisionGroups>,
        Local<'_, Vec<u64>>,
        Local<'_, Vec<u64>>,
    ),
) {
    let Some(grid) = grid else {
        return;
    };
    if relations.is_changed() || grid.is_changed() {
        groups.rebuild(grid.teams, &relations);
    }
    let words = grid.grid.cells().div_ceil(64);
    revealed.clear();
    revealed.resize(words * groups.count(), 0);
    detected.clear();
    detected.resize(words * groups.count(), 0);
    for (&pos, &team, sight, tags) in &seers {
        let run = groups.of(team) * words;
        let detects = UnitTags::effects_of(tags).detects();
        grid.grid.spans_within(pos, sight.range(), |cells| {
            set_bits(&mut revealed[run..run + words], cells.clone());
            if detects {
                set_bits(&mut detected[run..run + words], cells);
            }
        });
    }
    for (entity, &pos, &team, tags, seen) in &mut units {
        let mut teams = groups.members(groups.of(team));
        let cell = grid
            .grid
            .cell_of(pos)
            .expect("every unit stands within the bounds, which the grid covers");
        let hidden = UnitTags::effects_of(tags).hidden();
        let sight = if hidden { &detected } else { &revealed };
        for group in 0..groups.count() {
            if sight[group * words + cell / 64] & 1 << (cell % 64) != 0 {
                teams = teams.union(groups.members(group));
            }
        }
        match seen {
            Some(mut seen) if seen.get() != teams => *seen = SeenBy::new(teams),
            Some(_) => {}
            None => {
                commands.entity(entity).insert(SeenBy::new(teams));
            }
        }
    }
}

/// Sets the bits of `cells`, a run that is not empty, in `words`.
fn set_bits(words: &mut [u64], cells: Range<usize>) {
    debug_assert!(!cells.is_empty());
    let (first, last) = (cells.start / 64, (cells.end - 1) / 64);
    let from = u64::MAX << (cells.start % 64);
    let to = u64::MAX >> (63 - (cells.end - 1) % 64);
    if first == last {
        words[first] |= from & to;
    } else {
        words[first] |= from;
        words[first + 1..last].fill(u64::MAX);
        words[last] |= to;
    }
}

#[cfg(test)]
mod tests;
